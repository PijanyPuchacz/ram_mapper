use crate::fs::File;
use std::{
    fs,
    io::{self, BufRead},
};
use structopt::StructOpt;
use verbose_macros::verbose;

// Declare LB constants for k and n values, and if its fracturable
const LB_N: u32 = 10;
const LB_K: u32 = 6;
const LB_FRACT: bool = true;

// RAM to LB ratio constants
const LB_PER_18K: u32 = 30;
const LB_PER_64K: u32 = 100;

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

#[derive(Debug)]
struct ParsedRam {
    circuit_id: u32,
    ram_id: u32,
    mode: RamMode,
    depth: u32,
    width: u32,
}

#[derive(Debug)]
enum RamMode {
    ROM,
    SinglePort,
    SimpleDualPort,
    TrueDualPort,
}

enum RamType {
    LUTRAM,
    RAM18K,
    RAM64K,
}

struct RamMapping {
    id: u32,
    mode: RamMode,
    depth: u32,
    width: u32,
    lb_usage: u32,
    ram_type: RamType,
    serial: u32,
    parrallel: u32,
}

// Struct to contain all relevent info about a circuit and its mapped information
struct Circuit {
    id: u32,
    logic_lb_usage: u32,
    ram_mappings: Vec<RamMapping>,
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

    let rams = file_rams.lines();

    verbose!("Opening file: {}", opt.file_path_lb);

    // Open File and read lines for LBs
    let file_lb = io::BufReader::new(match File::open(&opt.file_path_lb) {
        Ok(file) => file,
        Err(err) => panic!("Error opening file: {err:?}"),
    });

    let lbs = file_lb.lines();

    let mut rams = rams.into_iter();
    rams.next();
    rams.next();

    println!("{:?}", ram_parse(rams.next().unwrap().unwrap().as_str()));

    // Mapping logic overview
    // First, Map to 18k and 64k as appropriate, if below a certain size just add to LUTRAM
    // Check resources usage, have we broken any rules for LB-BRAM ratios
    //          if yes then caluclate how much we need to move to LUTRAM to solve imbalance and adjust by
    //          finding appropriate RAMs to re-map
    // If other check passes, instead check if we have excessive LB usage and if we can move LUTRAMs to BRAMs
    //          if yes find suitable candedits and move to BRAMs based on availability until satisfied
    // end, print results to output file.
}

fn ram_parse(ram_string: &str) -> ParsedRam {
    let mut parsed_ram: ParsedRam = ParsedRam {
        circuit_id: 0,
        ram_id: 0,
        mode: RamMode::ROM,
        depth: 0,
        width: 0,
    };

    let mut string_split = ram_string.split("\t");

    //println!("{string_split:?}");

    parsed_ram.circuit_id = string_split.next().unwrap().parse::<u32>().unwrap();
    parsed_ram.ram_id = string_split.next().unwrap().parse::<u32>().unwrap();
    parsed_ram.mode = match string_split.next().unwrap() {
        "ROM" => RamMode::ROM,
        "SinglePort" => RamMode::SinglePort,
        "SimpleDualPort" => RamMode::SimpleDualPort,
        "TrueDualPort" => RamMode::TrueDualPort,
        _ => panic!("Error Parsing RAM Mode"),
    };
    parsed_ram.depth = string_split.next().unwrap().parse::<u32>().unwrap();
    parsed_ram.width = string_split.next().unwrap().parse::<u32>().unwrap();

    parsed_ram
}
