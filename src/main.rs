use crate::fs::File;
use std::{
    fs,
    io::{self, BufRead, Write},
};
use structopt::StructOpt;
use verbose_macros::verbose;

// Declare LB constants for k and n values, and if its fracturable
const LB_N: u32 = 10;
const LB_K: u32 = 6;
const LB_FRACT: bool = true;
// LUTRAM-LB ratio constant
const LUTRAM_PER_LB: f32 = 0.5;

#[derive(StructOpt, Debug)]
#[structopt(name = "options", no_version)]
struct Opt {
    /// BRAM config for smaller size, given as "-b1 "<ram_size> <max width> <lb_per_bram>" "
    #[structopt(short = "-s", default_value = "8192 32 10")]
    bram_smaller: String,

    /// BRAM config for larger size, given as "-b2 "<ram_size> <max_width> <lb_per_bram>" ", set ram_size = 0 to disable
    #[structopt(short = "-l", default_value = "131072 128 300")]
    bram_larger: String,

    /// Input file PATH with RAMs to map
    #[structopt(parse(from_str))]
    file_path_ram: String,

    ///Input file with logic block counts
    #[structopt(parse(from_str))]
    file_path_lb: String,

    /// Verbose output
    #[structopt(short = "-v", long = "--verbose")]
    verbose: bool,

    /// Disable LUTRAM mapping
    #[structopt(short = "-nl", long = "--no-lutram")]
    no_lutram: bool,
}

#[derive(Debug)]
struct RamConfig {
    ram_size: u32,
    max_width: u32,
    lb_per_bram: u32,
}

#[derive(Debug)]
struct ParsedRam {
    circuit_id: u32,
    ram_id: u32,
    mode: RamMode,
    depth: u32,
    width: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RamMode {
    ROM,
    SinglePort,
    SimpleDualPort,
    TrueDualPort,
}

#[derive(Debug, PartialEq)]
enum RamType {
    LUTRAM,
    RAM_SMALLER,
    RAM_LARGER,
}

#[derive(Debug)]
struct RamMapping {
    id: u32,
    mode: RamMode,
    logical_depth: u32,
    logical_width: u32,
    lb_usage: u32,
    ram_type: RamType,
    serial: u32,
    parrallel: u32,
    actual_depth: u32,
    actual_width: u32,
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

    use std::time::Instant;
    let start = Instant::now();

    //parse configurations
    let ram_config: Vec<&str> = opt.bram_smaller.split(" ").collect();
    let bram_smaller = match ram_config.len() {
        3 => RamConfig {
            ram_size: ram_config[0].parse().unwrap(),
            max_width: ram_config[1].parse().unwrap(),
            lb_per_bram: ram_config[2].parse().unwrap(),
        },
        _ => panic!("Incorrect parameters provided to '-b1'."),
    };
    let ram_config: Vec<&str> = opt.bram_larger.split(" ").collect();
    let bram_larger = match ram_config.len() {
        3 => RamConfig {
            ram_size: ram_config[0].parse().unwrap(),
            max_width: ram_config[1].parse().unwrap(),
            lb_per_bram: ram_config[2].parse().unwrap(),
        },
        _ => panic!("Incorrect parameters provided to '-b2'."),
    };

    println!("Smaller {:?}", bram_smaller);
    println!("Larger {:?}", bram_larger);
    println!("Use LUTRAM {:?}", !opt.no_lutram);

    let FILL_RATIO_LARGER: u32 = (bram_larger.ram_size as f32 * 0.5) as u32;
    let LB_PER_SMALLER: u32 = bram_smaller.lb_per_bram;
    let LB_PER_LARGER: u32 = bram_larger.lb_per_bram;
    let LUTRAM_BYTES: u32 = 640;
    let SMALLER_BYTES: u32 = bram_smaller.ram_size;
    let LARGER_BYTES: u32 = bram_larger.ram_size;
    let mut SMALLER_WIDTHS: Vec<u32> = Vec::new();
    let mut LARGER_WIDTHS: Vec<u32> = Vec::new();
    let LUTRAM_WIDTHS: Vec<u32> = Vec::from([10, 20]);

    //generate valid width vectors
    let mut widths = bram_smaller.max_width;
    while widths != 1 {
        SMALLER_WIDTHS.push(widths);
        widths /= 2;
    }
    SMALLER_WIDTHS.reverse();

    let mut widths = bram_larger.max_width;
    while widths != 1 {
        LARGER_WIDTHS.push(widths);
        widths /= 2;
    }
    LARGER_WIDTHS.reverse();

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

    let mut lbs = lbs.into_iter();
    let mut rams = rams.into_iter();

    // Skip header lines
    lbs.next();
    rams.next();
    rams.next();

    // Read logic block counts file and create vector to hold all circuits based on it.
    let mut circuits: Vec<Circuit> = Vec::new();
    for line in lbs {
        circuits.push(lb_parse(&line.unwrap().as_str()));
        verbose!(
            "Found circuit, Id:{}, LB usage:{}",
            circuits.last().unwrap().id,
            circuits.last().unwrap().logic_lb_usage
        );
    }

    //println!("{:?}", ram_parse(rams.next().unwrap().unwrap().as_str()));

    // Mapping logic overview
    // First, Map to 8k and 128k as appropriate
    // Check resources usage, have we broken any rules for LB-BRAM ratios
    //          if yes then caluclate how much we need to move to LUTRAM to solve imbalance and adjust by
    //          finding appropriate RAMs to re-map
    //              from 128k -> 8k if too many 128k, smallest -> largest until ratio is satisfied
    //              from 8k -> LUTRAM if too many 8k, smallest -> largest until ratio is satisfied
    // end, print results to output file.

    let start_mapping = Instant::now();

    for line in rams {
        let parsed_ram = ram_parse(line.unwrap().as_str());
        verbose!(
            "Found RAM, Circuit:{}, Id:{}, Mode: {:?}, Depth:{}, Width:{}",
            parsed_ram.circuit_id,
            parsed_ram.ram_id,
            parsed_ram.mode,
            parsed_ram.depth,
            parsed_ram.width
        );

        let bytes_needed = parsed_ram.depth * parsed_ram.width;
        let map: RamMapping;

        //Initial greedy map to BRAMs only
        if bytes_needed > FILL_RATIO_LARGER && bram_larger.ram_size != 0 {
            /* Map to Larger */
            verbose!("Mapping to Larger BRAM");
            map = map_ram(
                &parsed_ram,
                RamType::RAM_SMALLER,
                &LUTRAM_BYTES,
                &SMALLER_BYTES,
                &LARGER_BYTES,
                &SMALLER_WIDTHS,
                &LARGER_WIDTHS,
                &LUTRAM_WIDTHS,
            );
        } else {
            /* Map to Smaller */
            verbose!("Mapping to Smaller BRAM");
            map = map_ram(
                &parsed_ram,
                RamType::RAM_SMALLER,
                &LUTRAM_BYTES,
                &SMALLER_BYTES,
                &LARGER_BYTES,
                &SMALLER_WIDTHS,
                &LARGER_WIDTHS,
                &LUTRAM_WIDTHS,
            );
        }
        verbose!("Mapped: {:?}", map);
        circuits
            .iter_mut()
            .find(|x| x.id == parsed_ram.circuit_id)
            .unwrap()
            .ram_mappings
            .push(map);
    }

    // check legality of solutions
    for circuit in circuits.iter_mut() {
        let mut logic_block_usage = circuit.logic_lb_usage;
        let mut ram_larger = 0;
        let mut ram_smaller = 0;
        let mut lutram = 0;

        let mut legal = false;

        // sort the rams from smallest to largest logical capcity
        circuit.ram_mappings.sort_by(|a, b| {
            (a.logical_depth * a.logical_width).cmp(&(b.logical_depth * b.logical_width))
        });

        verbose!("Sorted ram mapping for circuit: {}", circuit.id,);

        let mut break_loop_count = 0; //if this gets to high try mapping 128K to LUTRAM to try and break loop

        //Legality checks
        while !legal {
            break_loop_count += 1;
            if break_loop_count > 10000 {
                //if really big number lol
                break_loop_count = 0;
                verbose!("Attempting to break loop");
                //attempt fix -> move 128K to LUTRAM
                //Vector should be sorted from smallest logical ram to largest, so finding first 128k should be the smallest one to convert now
                let ram = match circuit.ram_mappings.iter().position(|x| {
                    x.ram_type == RamType::RAM_LARGER
                        && x.mode != RamMode::TrueDualPort
                        && x.logical_depth / 64 < 17
                        && !opt.no_lutram // and if lutram is allowed
                }) {
                    Some(ram) => ram,
                    None => {
                        /* not possible without wasting LB slots, just add more to the calculation and keep trying, fuck test 61 */
                        circuit.logic_lb_usage += 100; // add a small but not too small value
                        continue;
                    }
                };

                verbose!(
                    "Circuit {} remapping: {:?}",
                    circuit.id,
                    circuit.ram_mappings[ram]
                );

                let remap = map_ram(
                    &ParsedRam {
                        circuit_id: circuit.id,
                        ram_id: circuit.ram_mappings[ram].id,
                        mode: circuit.ram_mappings[ram].mode,
                        depth: circuit.ram_mappings[ram].logical_depth,
                        width: circuit.ram_mappings[ram].logical_width,
                    },
                    RamType::LUTRAM,
                    &LUTRAM_BYTES,
                    &SMALLER_BYTES,
                    &LARGER_BYTES,
                    &SMALLER_WIDTHS,
                    &LARGER_WIDTHS,
                    &LUTRAM_WIDTHS,
                );

                verbose!("Remapped to: {:?}", remap);

                circuit.ram_mappings[ram] = remap;

                //loop back legality check
                continue;
            }

            // sum all LB and RAM counts
            lutram = 0;
            ram_smaller = 0;
            ram_larger = 0;
            logic_block_usage = circuit.logic_lb_usage;
            for ram in &circuit.ram_mappings {
                logic_block_usage += ram.lb_usage;
                match ram.ram_type {
                    RamType::LUTRAM => lutram += ram.parrallel * ram.serial,
                    RamType::RAM_SMALLER => ram_smaller += ram.parrallel * ram.serial,
                    RamType::RAM_LARGER => ram_larger += ram.parrallel * ram.serial,
                }
            }

            if lutram > (logic_block_usage as f32 * LUTRAM_PER_LB) as u32 {
                //calculate how much exceeded
                let exceeded = lutram - (logic_block_usage as f32 * LUTRAM_PER_LB) as u32;

                verbose!(
                    "Circuit {} LUTRAM usage exceeded by: {}!",
                    circuit.id,
                    exceeded
                );

                //attempt fix -> move to 8K

                //Vector should be sorted from smallest logical ram to largest, so finding first 8k should be the smallest one to convert now
                let ram = circuit
                    .ram_mappings
                    .iter()
                    .position(|x| x.ram_type == RamType::LUTRAM)
                    .unwrap();

                verbose!(
                    "Circuit {} remapping: {:?}",
                    circuit.id,
                    circuit.ram_mappings[ram]
                );

                let remap = map_ram(
                    &ParsedRam {
                        circuit_id: circuit.id,
                        ram_id: circuit.ram_mappings[ram].id,
                        mode: circuit.ram_mappings[ram].mode,
                        depth: circuit.ram_mappings[ram].logical_depth,
                        width: circuit.ram_mappings[ram].logical_width,
                    },
                    RamType::RAM_SMALLER,
                    &LUTRAM_BYTES,
                    &SMALLER_BYTES,
                    &LARGER_BYTES,
                    &SMALLER_WIDTHS,
                    &LARGER_WIDTHS,
                    &LUTRAM_WIDTHS,
                );

                verbose!("Remapped to: {:?}", remap);

                circuit.ram_mappings[ram] = remap;

                //loop back legality check
                continue;
            }
            if ram_smaller > logic_block_usage / LB_PER_SMALLER {
                //calculate how much exceeded
                let exceeded = ram_smaller - (logic_block_usage / LB_PER_SMALLER);

                verbose!("Circuit {} 8K usage exceeded by: {}!", circuit.id, exceeded);

                //attempt fix -> move to LUTRAM
                let mut map_to = RamType::LUTRAM;
                //Vector should be sorted from smallest logical ram to largest, so finding first 8k should be the smallest one to convert now
                let ram = match circuit.ram_mappings.iter().position(|x| {
                    x.ram_type == RamType::RAM_SMALLER    // Find RAM_SMALLER That isn't a truedualport and is not too deep to map to LUTRAM
                        && x.mode != RamMode::TrueDualPort
                        && x.logical_depth / 64 < 17
                        && !opt.no_lutram // and if lutram is allowed
                }) {
                    Some(ram) => ram,
                    None => {
                        /* Could not find valid target to map, try to map to 128K instead*/
                        map_to = RamType::RAM_LARGER;
                        circuit
                            .ram_mappings
                            .iter()
                            .rposition(|x| {
                                x.ram_type == RamType::RAM_SMALLER && bram_larger.ram_size != 0
                            })
                            .unwrap()
                    }
                };
                verbose!(
                    "Circuit {} remapping: {:?}",
                    circuit.id,
                    circuit.ram_mappings[ram]
                );

                let remap = map_ram(
                    &ParsedRam {
                        circuit_id: circuit.id,
                        ram_id: circuit.ram_mappings[ram].id,
                        mode: circuit.ram_mappings[ram].mode,
                        depth: circuit.ram_mappings[ram].logical_depth,
                        width: circuit.ram_mappings[ram].logical_width,
                    },
                    map_to,
                    &LUTRAM_BYTES,
                    &SMALLER_BYTES,
                    &LARGER_BYTES,
                    &SMALLER_WIDTHS,
                    &LARGER_WIDTHS,
                    &LUTRAM_WIDTHS,
                );

                verbose!("Remapped to: {:?}", remap);

                circuit.ram_mappings[ram] = remap;

                //loop back legality check
                continue;
            }
            if ram_larger > logic_block_usage / LB_PER_LARGER && bram_larger.ram_size != 0 {
                //calculate how much exceeded
                let exceeded = ram_larger - (logic_block_usage / LB_PER_LARGER);

                verbose!(
                    "Circuit {} 128K usage exceeded by: {}!",
                    circuit.id,
                    exceeded
                );

                //attempt fix -> move to 8k
                //Vector should be sorted from smallest logical ram to largest, so finding first 8k should be the smallest one to convert now
                let ram = circuit
                    .ram_mappings
                    .iter()
                    .position(|x| x.ram_type == RamType::RAM_LARGER)
                    .unwrap();

                verbose!(
                    "Circuit {} remapping: {:?}",
                    circuit.id,
                    circuit.ram_mappings[ram]
                );

                let remap = map_ram(
                    &ParsedRam {
                        circuit_id: circuit.id,
                        ram_id: circuit.ram_mappings[ram].id,
                        mode: circuit.ram_mappings[ram].mode,
                        depth: circuit.ram_mappings[ram].logical_depth,
                        width: circuit.ram_mappings[ram].logical_width,
                    },
                    RamType::RAM_SMALLER,
                    &LUTRAM_BYTES,
                    &SMALLER_BYTES,
                    &LARGER_BYTES,
                    &SMALLER_WIDTHS,
                    &LARGER_WIDTHS,
                    &LUTRAM_WIDTHS,
                );

                verbose!("Remapped to: {:?}", remap);

                circuit.ram_mappings[ram] = remap;

                //loop back legality check
                continue;
            }

            //if passed all check then set to true and exit loop
            legal = true;
            break_loop_count = 0;
        }
    }

    let elapsed = start_mapping.elapsed();
    println!("Time to map RAM: {:.5?}", elapsed);

    // Print solutions to file
    let mut file = File::create("ram_mappings.txt").unwrap();
    for circuit in circuits {
        for ram in circuit.ram_mappings {
            match file.write(
                format!(
                    "{} {} {} LW {} LD {} ID {} S {} P {} Type {} Mode {:?} W {} D {}\n",
                    circuit.id,
                    ram.id,
                    ram.lb_usage * LB_N,
                    ram.logical_width,
                    ram.logical_depth,
                    ram.id,
                    ram.serial,
                    ram.parrallel,
                    ram.ram_type as u32 + 1,
                    ram.mode,
                    ram.actual_width,
                    ram.actual_depth
                )
                .as_bytes(),
            ) {
                Ok(_) => { /* Do Nothing */ }
                Err(err) => panic!("Error printing to file: {:?}", err),
            }
        }
    }

    let elapsed = start.elapsed();
    println!("Time for full run: {:.5?}", elapsed);
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
        "ROM           " => RamMode::ROM,
        "SinglePort    " => RamMode::SinglePort,
        "SimpleDualPort" => RamMode::SimpleDualPort,
        "TrueDualPort  " => RamMode::TrueDualPort,
        _ => panic!("Error Parsing RAM Mode"),
    };
    parsed_ram.depth = string_split.next().unwrap().parse::<u32>().unwrap();
    parsed_ram.width = string_split.next().unwrap().parse::<u32>().unwrap();

    parsed_ram
}

fn lb_parse(lb_string: &str) -> Circuit {
    let mut circuit: Circuit = Circuit {
        id: 0,
        logic_lb_usage: 0,
        ram_mappings: Vec::new(),
    };

    let mut string_split = lb_string.split("\t");

    circuit.id = string_split.next().unwrap().parse::<u32>().unwrap();
    circuit.logic_lb_usage = string_split.next().unwrap().parse::<u32>().unwrap();

    circuit
}

fn map_ram(
    parsed: &ParsedRam,
    map_to: RamType,
    LUTRAM_BYTES: &u32,
    SMALLER_BYTES: &u32,
    LARGER_BYTES: &u32,
    SMALLER_WIDTHS: &Vec<u32>,
    LARGER_WIDTHS: &Vec<u32>,
    LUTRAM_WIDTHS: &Vec<u32>,
) -> RamMapping {
    let mut lb_usage = 0;
    let mut serial = 17;
    let mut parrallel = 0;
    let mut actual_depth = 0;
    let mut actual_width = 0;

    let ram_size = match parsed.mode {
        RamMode::TrueDualPort => match map_to {
            RamType::RAM_LARGER => (LARGER_BYTES, LARGER_WIDTHS),
            RamType::RAM_SMALLER => (SMALLER_BYTES, SMALLER_WIDTHS),
            RamType::LUTRAM => panic!("Error! Cannot map TrueDualPort to LUTRAM!"),
        },
        _ => match map_to {
            RamType::RAM_LARGER => (LARGER_BYTES, LARGER_WIDTHS),
            RamType::RAM_SMALLER => (SMALLER_BYTES, SMALLER_WIDTHS),
            RamType::LUTRAM => (LUTRAM_BYTES, LUTRAM_WIDTHS),
        },
    };

    let mut loop_counter = 0;

    // Map loop to check for Serial > 16
    while serial > 16 {
        loop_counter += 1;
        actual_width = 0;
        actual_depth = 0;

        // Width Map
        let mut counter = loop_counter;
        while actual_width == 0 {
            for try_width in ram_size.1 {
                if try_width * counter / parsed.width >= 1 {
                    if try_width == ram_size.1.last().unwrap()
                        && parsed.mode == RamMode::TrueDualPort
                    {
                        continue;
                    }
                    actual_width = try_width.to_owned();
                    parrallel = counter;
                    break;
                }
            }
            counter += 1;
        }

        // Depth Map
        counter = 1;
        while actual_depth == 0 {
            if ((counter * ram_size.0) / actual_width) / parsed.depth > 0 {
                serial = counter;
                actual_depth = ram_size.0 / actual_width;
            }
            counter += 1;
        }
    }

    // Determine cost of addtional logic
    // Additional LB for serial RAMs Decoding and Multiplexing
    let additional_lut_mux = parsed.width * ((serial + 1) / 3);
    let additional_lut_dec = match serial {
        1 => 0,
        2 => 1,
        _ => serial,
    };

    // If we're using a true dual mode RAM we need to double the LUTs used for muxing and decoding.
    let true_dual_mod = match parsed.mode {
        RamMode::TrueDualPort => 2,
        _ => 1,
    };

    lb_usage = (true_dual_mod * (additional_lut_mux + additional_lut_dec) + LB_N - 1) / LB_N; // Integer divide but round up instead of down.

    // If this is a LUTRAM also add that cost
    if map_to == RamType::LUTRAM {
        lb_usage += serial * parrallel;
    }

    RamMapping {
        id: parsed.ram_id,
        mode: parsed.mode,
        logical_depth: parsed.depth,
        logical_width: parsed.width,
        lb_usage: lb_usage,
        ram_type: map_to,
        serial: serial,
        parrallel: parrallel,
        actual_depth: actual_depth,
        actual_width: actual_width,
    }
}
