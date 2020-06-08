![Build](https://github.com/xermicus/blindspot/workflows/Build/badge.svg?branch=master)
[![Cargo](https://img.shields.io/crates/v/blindspot.svg)](https://crates.io/crates/blindspot)
[![License: MIT](https://img.shields.io/badge/License-MIT-red.svg)](https://opensource.org/licenses/MIT)

# The BlindSpot Package Manager
Most of the software running on my linux computers are managed by official or community maintained repositories or various 3rd-party package managers.
However, especially for newer software projects it can take a while for a package to become available even when using the most popular distros.
Additionally, a tendency I started to notice about projects built with a language producing statically linked binaries:

> Because all you need is our binary somewhere inside `$PATH` anyways, just download this artifact here directly from our CI builds and you're good

I'm generally fine with this installation method, but it creates a problem: These binaries are not managed by anything on my system and therefore remembering when and how to update is cumbersome. They kind of live in a blind spot of my package manager(s). And this is how the idea for `blindspot` started! See it in action:

[![asciicast](https://asciinema.org/a/337585.svg)](https://asciinema.org/a/337585)

# Features
* Install a package based on a browser download URL
* Detect GitHub repos and install from GitHub release asset
* Detect tar archives and common compression based on the filename and guide through extracting files
* Update packages simultaneously
* Revert a package to the previous version from before the update
* Uses user local standard directories for data and configuration, no root privileges required
* It's fast and has lots of emojis in the user interface

# Installation
## Github release
Download a [release](https://github.com/xermicus/blindspot/releases) and run the `init` command that can install itself:
```bash
cd ~/Downloads # assuming you downloaded it there
chmod +x blindspot
./blindspot init
rm ./blindspot
```
This automatically creates the config file and installs `blindspot` into the current users local bin dir.

## Cargo
```bash
cargo install blindspot
blindspot init --no-install
```

# Usage
The usage should not be too far off from what you'd expect from a package manager. View the [asciinema](https://asciinema.org/a/337585) to get the basic idea.

## Help
Use the `--help` flag to learn about the various subcommands.

## Configuration
`blindspot` works out of the box if at least your `$HOME` env var is set. Use the following environment variables to overwrite default behaviour:

|Name|Purpose|Default|
|-|-|-|
|**$BSPM_CONFIG**|Location of the config file|`$XDG_CONFIG_HOME/blindspot/bspm.yaml` or `$HOME/.config/blindspot/bspm.yaml`|
|**$BSPM_BIN_DIR**|Where application binaries get installed to|`$XDG_BIN_HOME/../bin` or `$XDG_DATA_HOME/../bin` or `$HOME/.local/bin`|
|**$BSPM_DATA_DIR**|Where backup binaries for a rollbacks are kept|`$XDG_DATA_HOME/blindspot/` or `$HOME/.local/share/blindspot`|

## Shell completion
Completions for the most popular shells are provided. Default is `bash`:
```bash
blindspot completion >> ~/.bash_profile
. ~/.bash_profile
```

# Disclaimer
Do not run this software as `root`! There's should be no reason to do so.

This tool is just a small hobby project and in no way trying to solve package management on linux as a whole.
