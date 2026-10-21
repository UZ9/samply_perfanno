use std::{path::PathBuf, sync::Arc};

use anyhow::{Result, anyhow};
use clap::Parser;
use wholesym::{SymbolMap, VerboseSymbolManagerObserver};

use crate::{
    json_loader::load_firefox_profile,
    types::{Profile, Thread},
};

mod json_loader;
mod types;

// Rough strategy for conversion:
// -- 1 gunzip profile.json.gz --
//
// things we need:
// - libs/ -> Lib[] binary paths and dylib, will need to load these as symbol maps
//

fn parse_hex(s: &str) -> Result<u32, String> {
    let src = s.strip_prefix("0x").unwrap_or(s);

    u32::from_str_radix(src, 16).map_err(|e| format!("{} bad hex", e))
}

#[derive(Parser)]
struct Args {
    profile: PathBuf,

    exe: PathBuf,

    #[clap(short, long, value_parser=parse_hex)]
    addr: u32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Args::parse();

    let config = wholesym::SymbolManagerConfig::new()
        .use_spotlight(true)
        .respect_nt_symbol_path(true);

    let mut symbol_manager = wholesym::SymbolManager::with_config(config);

    // TODO: need to load for all libs not just main executable
    let path: PathBuf = cli.exe;

    // symbol_manager.set_observer(Some(Arc::new(VerboseSymbolManagerObserver::new())));

    let symbol_map = symbol_manager
        .load_symbol_map_for_binary_at_path(&path, None)
        .await?;

    print!("{:#x}: ", cli.addr);

    let profile = load_firefox_profile(&cli.profile)?;

    for thread in &profile.threads {
        let stack_counts = get_stack_counts(thread);

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

            let mut curr_idx = i;

            let mut callstack_strs: Vec<String> = Vec::new();

            while let Some(prefix) = thread.stack_table.prefix[curr_idx] {
                let frame_idx = thread.stack_table.frame[curr_idx];

                match frame_name_line(&symbol_map, thread, frame_idx).await {
                    Ok((function, line)) => {
                        if let Some(line) = line {
                            let dis = function + ":" + &line.to_string();
                            println!("found {dis} for stack {curr_idx}");
                            callstack_strs.push(dis);
                        }
                    }
                    Err(_) => {
                        curr_idx = prefix;
                        continue;
                    }
                }

                curr_idx = prefix;
            }

            println!("REACHED THE END");

            // last frame
            let frame_idx = thread.stack_table.frame[curr_idx];

            match frame_name_line(&symbol_map, thread, frame_idx).await {
                Ok((function, line)) => {
                    if let Some(line) = line {
                        let dis = function + ":" + &line.to_string();
                        println!("found {dis} for stack {curr_idx}");
                        callstack_strs.push(dis);
                    }
                }
                Err(_) => {}
            }

            callstack_strs.reverse();

            println!("completed stack: {:?}", callstack_strs);
        }

        println!("stack_counts: {:?}", stack_counts);

        // for (i, address) in thread.frame_table.address.iter().enumerate() {
        //     // For now we need to filter this to just be our executable. Given a frame at [i] within the frame
        //     // table, we can reference this via func idx -> func table -> resource table
        //
        //     let func_idx = thread.frame_table.func[i];
        //     let res_idx = thread.func_table.resource[func_idx];
        //
        //     if res_idx == -1 {
        //         continue;
        //     }
        //
        //     if let Some(lib_idx) = thread.resource_table.lib[res_idx as usize].as_ref() {
        //         let lib = &profile.libs[*lib_idx];
        //
        //         if lib.path == "./target/release/one-billion-rows" {
        //             print_addr(*address, &symbol_map).await;
        //         }
        //     }
        // }
    }

    Ok(())
}

async fn frame_name_line(
    symbol_map: &SymbolMap,
    thread: &Thread,
    idx: usize,
) -> Result<(String, Option<u32>)> {
    let addr = thread.frame_table.address[idx];

    if addr == -1 {
        println!("{addr}:-1");
    }

    let Some(address_info) = symbol_map
        .lookup(wholesym::LookupAddress::Relative(addr as u32))
        .await
    else {
        eprintln!("failed to find symbol for {:#x}", addr);
        return Err(anyhow!("addr invalid"));
    };

    if let Some(frames) = address_info.frames {
        for frame in frames {
            let Some(path) = frame.file_path else {
                eprintln!(
                    "Failed to find frame file_path for addr {addr}, do we have debug symbols enabled?"
                );
                continue;
            };

            // let sample_display = match frame.function {
            //     Some(function) => path.display_path() + ":" + &function,
            //     None => path.display_path()
            // };

            return Ok((path.display_path(), frame.line_number));
        }
    } else {
        return Err(anyhow!(
            "Failed to fetch frames for addr {addr}, do we have debug symbols enabled?"
        ));
    }

    return Err(anyhow!(
        "Found no frame with file_path, do we have debug symbols enabled?"
    ));
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

async fn print_addr(addr: i64, symbol_map: &SymbolMap) {
    if addr == -1 {
        println!("{addr}:-1");
    }

    let Some(address_info) = symbol_map
        .lookup(wholesym::LookupAddress::Relative(addr as u32))
        .await
    else {
        eprintln!("failed to find symbol for {:#x}", addr);
        return;
    };

    if let Some(frames) = address_info.frames {
        for frame in frames {
            let Some(path) = frame.file_path else {
                eprintln!(
                    "Failed to find frame file_path for addr {addr}, do we have debug symbols enabled?"
                );
                continue;
            };

            match (frame.line_number, frame.function) {
                (Some(line_number), Some(function)) => {
                    println!("{}:{function}:{}", path.display_path(), line_number);
                }
                (Some(line_number), None) => {
                    println!("{}:{}", path.display_path(), line_number);
                }
                _ => {
                    println!("{}", path.display_path());
                }
            }
        }
    } else {
        eprintln!("Failed to fetch frames for addr {addr}, do we have debug symbols enabled?");
    }
}
