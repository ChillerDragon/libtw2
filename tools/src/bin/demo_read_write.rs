use clap::App;
use clap::Arg;
use libtw2_demo::ddnet;
use libtw2_gamenet_ddnet::Protocol as DDNet;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::io::BufWriter;
use std::process;

fn main() {
    libtw2_logger::init();
    let matches = App::new("Teehistorian reader")
        .about(
            "Reads teehistorian file and dumps its contents in a human-readable\
                text stream",
        )
        .arg(
            Arg::with_name("INPUT_DEMO")
                .help("Sets the demo file to read")
                .required(true),
        )
        .arg(
            Arg::with_name("OUTPUT_DEMO")
                .help("Sets the path to write to")
                .required(true),
        )
        .arg(
            Arg::with_name("DDNET")
                .long("ddnet")
                .help("Interpret the demo as a DDNet demo"),
        )
        .get_matches();

    let input = matches.value_of("INPUT_DEMO").unwrap();
    let output = matches.value_of("OUTPUT_DEMO").unwrap();
    let as_ddnet = matches.is_present("DDNET");
    let rewrite = match as_ddnet {
        true => ddnet_read_write,
        false => read_write,
    };
    if let Err(err) = rewrite(input, output) {
        println!("Error: {}", err);
        process::exit(-1);
    }
}

fn read_write(input: &str, output: &str) -> Result<(), Box<dyn Error>> {
    let input_file = BufReader::new(File::open(input)?);
    let output_file = BufWriter::new(File::create(output)?);
    let mut reader = libtw2_demo::Reader::new(input_file, &mut warn::Ignore)?;
    let mut writer = libtw2_demo::Writer::new(
        output_file,
        reader.net_version(),
        reader.map_name(),
        reader.map_sha256(),
        reader.map_crc(),
        reader.kind(),
        reader.length(),
        reader.timestamp(),
        reader.map_data(),
    )?;
    while let Some(chunk) = reader.read_chunk(&mut warn::Ignore)? {
        writer.write_chunk(chunk)?;
    }
    Ok(())
}

fn ddnet_read_write(input: &str, output: &str) -> Result<(), Box<dyn Error>> {
    let input_file = BufReader::new(File::open(input)?);
    let output_file = BufWriter::new(File::create(output)?);
    let mut reader = ddnet::DemoReader::<DDNet>::new(input_file, &mut warn::Log)?;
    let mut writer = ddnet::DemoWriter::<DDNet>::new(
        output_file,
        reader.net_version(),
        reader.map_name(),
        reader.map_sha256(),
        reader.map_crc(),
        reader.kind(),
        reader.length(),
        reader.timestamp(),
        reader.map_data(),
    )?;
    let mut last_tick = None;
    while let Some(chunk) = reader.next_chunk(&mut warn::Log)? {
        match chunk {
            ddnet::Chunk::Message(msg) => writer.write_msg(&msg)?,
            ddnet::Chunk::Snapshot(snap) => match last_tick.take() {
                None => eprintln!("Snapshot without tick"),
                // HACK: `.rev()` so that higher-valued items get inserted
                // first, so that the extended item types get inserted in the
                // same order as they were in the original demo.
                Some(t) => writer.write_snap(t, snap.rev().map(|(obj, id)| (obj, *id)))?,
            },
            ddnet::Chunk::Tick(t) => last_tick = Some(t),
            ddnet::Chunk::Invalid => eprintln!("Invalid chunk!"),
        }
    }
    Ok(())
}
