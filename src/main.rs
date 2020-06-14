#[macro_use]
extern crate serde_derive;
use structopt::StructOpt;

use cmd::Command;

mod bspm;
mod cmd;

fn main() -> anyhow::Result<()> {
    smol::run(async { Command::from_args().go().await })
}
