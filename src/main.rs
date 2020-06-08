#[macro_use]
extern crate serde_derive;
use structopt::StructOpt;

use cmd::Command;

mod cmd;
mod bspm;

fn main() -> anyhow::Result<()> {
    smol::run(async {
        Command::from_args()
            .go()
            .await
    })
}
