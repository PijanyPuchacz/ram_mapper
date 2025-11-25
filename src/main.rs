use crate::fs::File;
use std::{fs, io};
use structopt::StructOpt;
use verbose_macros::verbose;

// Declare LB constants for k and n values, and if its fracturable
const LB_N: u32 = 10;
const LB_K: u32 = 6;
const LB_FRACT: bool = true;

#[derive(StructOpt, Debug)]
#[structopt(name = "options", no_version)]
struct Opt {
    /// Input file PATH with RAMs to map
    #[structopt(parse(from_str))]
    file_path_ram: String,

    ///Input file with logic block counts
    #[structopt(parse(from_str))]
    file_path_lb: String,

    /// Verbose output
    #[structopt(short = "-v", long = "--verbose")]
    verbose: bool,
}

enum RamMode {
    ROM,
    SinglePort,
    SimpleDualPort,
    TrueDualPort
}

struct RamMapping {
    id: u32,
    mode: RamMode,
    depth: u32,
    width: u32
}

// Struct to contain all relevent info about a circuit and its mapped information
struct Circuit {
    logic_lb_usage: u32,
    ram_mappings:
}

fn main() {
    let opt = Opt::from_args();
    verbose_macros::set_verbose(opt.verbose);

    verbose!("Opening file: {}", opt.file_path_ram);

    // Open File and read lines for RAM
    let file_rams = io::BufReader::new(match File::open(&opt.file_path_ram) {
        Ok(file) => file,
        Err(err) => panic!("Error opening file: {err:?}"),
    });

    verbose!("Opening file: {}", opt.file_path_lb);

    // Open File and read lines for LBs
    let file_lb = io::BufReader::new(match File::open(&opt.file_path_lb) {
        Ok(file) => file,
        Err(err) => panic!("Error opening file: {err:?}"),
    });
}
