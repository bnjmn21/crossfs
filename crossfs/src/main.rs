use std::{
    net::{IpAddr, Ipv4Addr},
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use clap::Parser;
use crossfs::{
    CrossFs, SimBackend,
    msfs::MsfsSim,
    net::{Client, server},
    xplane::XPlaneSim,
};
use rand::RngExt;

const TICK_INTERVAL: Duration = Duration::from_millis(1000 / 60);

fn main() -> ExitCode {
    let args = Cli::parse();
    let is_client = args.xplane || args.msfs;
    if !args.serve && !is_client {
        println!("No option selected.");
        return ExitCode::FAILURE;
    }
    if args.xplane as usize + args.msfs as usize > 1 {
        println!("More than one sim selected.");
        return ExitCode::FAILURE;
    }

    match (args.serve, is_client) {
        (true, true) => {
            if args.password.is_some() {
                println!("The --password argument cannot be used with --serve.");
                return ExitCode::FAILURE;
            }
            let password = rand::rng().random();
            println!("Password: {password}");
            let ready: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
            let ready2 = ready.clone();
            thread::spawn(move || {
                server(
                    IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                    args.serve_port,
                    password,
                    || {
                        ready2.store(true, Ordering::Relaxed);
                    },
                )
            });
            while !ready.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(10));
            }
            client(
                SimType::from_args(&args).unwrap(),
                &format!("127.0.0.1:{}", args.serve_port),
                password,
                args.master,
            );
        }
        (true, false) => {
            if args.password.is_some() {
                println!("The --password argument cannot be used with --serve.");
                return ExitCode::FAILURE;
            }
            let password = rand::rng().random();
            println!("Password: {password}");
            server(
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                args.serve_port,
                password,
                || (),
            )
        }
        (false, true) => {
            if args.password.is_none() {
                println!("Password must be specified.");
                return ExitCode::FAILURE;
            }
            if args.address.is_none() {
                println!("Server address must be specified.");
                return ExitCode::FAILURE;
            }
            client(
                SimType::from_args(&args).unwrap(),
                &args.address.unwrap(),
                args.password.unwrap(),
                args.master,
            );
        }
        (false, false) => unreachable!(),
    }

    ExitCode::SUCCESS
}

fn client(sim_type: SimType, addr: &str, password: u64, master: bool) {
    println!("Starting client...");
    let tcp_client = Client::new(addr, password, master).unwrap();
    println!("Connected to server.");
    let sim: Box<dyn SimBackend> = match sim_type {
        SimType::XPlane => Box::new(XPlaneSim::new()),
        SimType::Msfs => Box::new(MsfsSim::new().unwrap()),
    };
    let mut crossfs = CrossFs::new(sim, tcp_client, master);
    loop {
        crossfs.tick().unwrap();
        thread::sleep(TICK_INTERVAL);
    }
}

enum SimType {
    XPlane,
    Msfs,
}

impl SimType {
    fn from_args(args: &Cli) -> Option<SimType> {
        if args.xplane {
            Some(SimType::XPlane)
        } else if args.msfs {
            Some(SimType::Msfs)
        } else {
            None
        }
    }
}

/// Connect your flight sim with others and let people join your cockpit
#[derive(Parser)]
#[command(long_about = r#"
Connect your flight sim with others and let people join your cockpit.

Examples:
- Connect to a server with MSFS24:
  > crossfs --msfs --address=123.456.78.9:26000 --password=13245678
- Connect to a server with MSFS24 as master:
  > crossfs --msfs --address=123.456.78.9:26000 --password=13245678 --master
- Run the server:
  > crossfs --serve
- Run the server and connect to it with X-Plane:
  > crossfs --serve --xplane
"#)]
struct Cli {
    /// Connect to a server with X-Plane (requires CrossFS X-Plane plugin)
    #[arg(long)]
    xplane: bool,
    /// Connect to a server with MSFS
    #[arg(long)]
    msfs: bool,
    /// Address of the server to connect to. Can be left empty if --serve is specified
    #[arg(short, long)]
    address: Option<String>,
    /// Connect as a master
    #[arg(long)]
    master: bool,
    /// Run the CrossFS Server
    #[arg(long)]
    serve: bool,
    /// Password of the server
    #[arg(short, long)]
    password: Option<u64>,
    /// The port the server should use
    #[arg(long, default_value = "26000")]
    serve_port: u16,
}
