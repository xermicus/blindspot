#[macro_use]
extern crate serde_derive;
use structopt::StructOpt;

use cmd::Command;

mod bspm;
mod cmd;

fn main() -> anyhow::Result<()> {
    smol::block_on(async { Command::from_args().go().await })
}
