use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::Parser;
use wholesym::{MultiArchDisambiguator, SymbolManager, SymbolMap};

use crate::{
    json_loader::load_firefox_profile,
    types::{Profile, Thread},
};

mod json_loader;
mod types;

#[derive(Parser)]
struct Args {
    profile: PathBuf,

    exe: PathBuf,

    // TODO: have to include path of where samply was ran because of relative paths, might be an
    // easier way of autodetecting this
    samply_path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Args::parse();

    let config = wholesym::SymbolManagerConfig::new()
        .use_spotlight(true)
        .respect_nt_symbol_path(true);

    let symbol_manager = wholesym::SymbolManager::with_config(config);

    let root = &cli.samply_path;

    let mut symbol_maps: HashMap<usize, Option<SymbolMap>> = HashMap::new();

    let profile = load_firefox_profile(&cli.profile)?;

    for thread in &profile.threads {
        let stack_counts = get_stack_counts(thread);

        // We do end up getting a bunch of duplicate lines, so we'll keep track and sum counts
        let mut stacktrace: HashMap<String, u32> = HashMap::new();

        for (i, stack) in thread.stack_table.frame.iter().enumerate() {
            // TODO: will find a significantly more optimized way of traversing this later,
            // but for now:
            // - For each stack we know our frame and the stack before this (where that stack might
            // be multiple frames)
            // - e.g. A -> B -> C, (A -> B) would be prefix, C would be our frame
            // - if it was just A prefix would be None
            // For a flamegraph format perfanno wants we need:
            // /path/to/src_1.cpp:30;/path/to/src_2.cpp:27;/path/to/src_1.cpp:27 47
            // from https://github.com/t-troebst/perfanno.nvim README

            // Rough process:
            // - Get stack sample count from stack table
            // - Traverse stack table until prefix is None
            // - While we still have a frame to process:
            //  - Fetch frame name + addr from frame table
            let stack_count = stack_counts[i];

            // For flamegraphs we don't need to care about any stack not called
            if stack_count == 0 {
                continue;
            }

            let mut curr = Some(i);

            let mut callstack_strs: Vec<String> = Vec::new();

            while let Some(idx) = curr {
                let frame_idx = thread.stack_table.frame[idx];

                callstack_strs.extend(
                    frame_name_line(
                        &mut symbol_maps,
                        &symbol_manager,
                        &profile,
                        thread,
                        frame_idx,
                        root,
                    )
                    .await,
                );
                curr = thread.stack_table.prefix[idx];
            }

            callstack_strs.reverse();

            let func_key = callstack_strs.join(";");
            let entry = stacktrace.entry(func_key);

            let count: &mut u32 = entry.or_insert(0);

            *count += stack_count as u32;
        }

        let mut lines: Vec<_> = stacktrace.into_iter().collect();

        for (stack, count) in lines {
            println!("{stack} {count}");

        }
    }


    Ok(())
}

async fn frame_name_line(
    maps: &mut HashMap<usize, Option<SymbolMap>>,
    manager: &SymbolManager,
    profile: &Profile,
    thread: &Thread,
    idx: usize,
    root: &Path,
) -> Vec<String> {
    let mut ret = Vec::new();

    let addr = thread.frame_table.address[idx];
    let func = thread.frame_table.func[idx];
    let resource = thread.func_table.resource[func];

    if resource == -1 || addr == -1 {
        return vec![format!("unknown+{addr:#x}")];
    }

    let Some(lib_idx) = thread.resource_table.lib[resource as usize] else {
        return vec!["lib_idx None".to_string()];
    };

    if !maps.contains_key(&lib_idx) {
        let lib = &profile.libs[lib_idx];
        let path = root.join(lib.debug_path.as_deref().unwrap_or(&lib.path));

        // Hurrah macOS! Need to do additional disambiguation given breakpads otherwise we fail on
        // all map indexes, noted in load_symbol_map_for_binary_at_path docs
        let multiarch_disambiguator = lib
            .breakpad_id
            .as_deref()
            .and_then(|id| wholesym::debugid::DebugId::from_breakpad(id).ok())
            .map(MultiArchDisambiguator::DebugId);

        let mut res = manager.load_symbol_map_for_binary_at_path(&path, multiarch_disambiguator).await;

        if res.is_err() {
            res = manager.load_symbol_map_for_binary_at_path(&path, lib.arch.clone().map(MultiArchDisambiguator::Arch)).await;
        }

        if res.is_err() {
            res = manager.load_symbol_map_for_binary_at_path(&path, None).await;
        }

        let map = match  res 
        {
            Ok(m) => Some(m),
            Err(e) => {
                eprintln!("couldn't load symbols for lib {lib_idx} {e}");
                None
            }
        };

        if map.is_none() {
            eprintln!("bad symbol");
        }

        maps.insert(lib_idx, map);
    }

    let Some(map) = maps[&lib_idx].as_ref() else {
        return vec![format!("{}+{addr:#x}", profile.libs[lib_idx].path.rsplit("/").next().unwrap())];
    };
    let Some(info) = map
        .lookup(wholesym::LookupAddress::Relative(addr as u32))
        .await
    else {
        return vec![format!("{}+{addr:#x}", profile.libs[lib_idx].path.rsplit("/").next().unwrap())];
    };

    if let Some(frames) = info.frames {
        for frame in frames {
            let sample_function_name = frame.function.unwrap();

            match (frame.file_path, frame.line_number) {
                (Some(file_path), Some(line)) => {
                    ret.push(format!("{}:{}", file_path.display_path(), line))
                }
                _ => ret.push(sample_function_name),
            };
        }
    }

    if ret.len() == 0 {
        ret.push(info.symbol.name.clone());
    }

    ret
}

fn get_stack_counts(thread: &Thread) -> Vec<u32> {
    // We need to count the number of samples hitting a particular stack (i.e. find where processing
    // time is being spent). We already know the number of stacks via the stack_table, so we'll just
    // track the counts via a Vec of size stack_table.frame.len()
    let mut counters: Vec<u32> = vec![0; thread.stack_table.frame.len()];

    let sample_table = &thread.samples;

    for sample in &sample_table.stack {
        // Sample can be None if we aren't running anything
        if let Some(sample) = sample {
            counters[*sample] += 1;
        }
    }

    counters
}
