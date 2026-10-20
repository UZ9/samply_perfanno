use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::{Parser, error::Result};
use wholesym::VerboseSymbolManagerObserver;

use crate::json_loader::load_firefox_profile;

mod types;
mod json_loader;

// Rough strategy for conversion:
// -- 1 gunzip profile.json.gz --
//
// things we need:
// - libs/ -> Lib[] binary paths and dylib, will need to load these as symbol maps
//      
//
//
//
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

    let path: PathBuf = cli.exe;

    symbol_manager.set_observer(Some(Arc::new(VerboseSymbolManagerObserver::new())));

    let symbol_map = symbol_manager
        .load_symbol_map_for_binary_at_path(&path, None)
        .await?;

    print!("{:#x}: ", cli.addr);

    let profile = load_firefox_profile(&cli.profile)?;

    println!("{:?}", profile.libs);

    if let Some(address_info) = symbol_map
        .lookup(wholesym::LookupAddress::Relative(cli.addr))
        .await
    {
        if let Some(frames) = address_info.frames {
            for frame in frames {
                if let Some(path) = frame.file_path {
                    if let Some(line_number) = frame.line_number {
                        if let Some(function) = frame.function {
                            println!("{}:{function}:{}", path.display_path(), line_number);
                        } else {
                            println!("{}:{}", path.display_path(), line_number);
                        }
                    } else {
                        println!("{}", path.display_path());
                    }
                }
            }
        }
    } else {
        eprintln!("failed to find symbol for {:#x}", cli.addr);
    }

    println!("Hello, world!");

    Ok(())
}
