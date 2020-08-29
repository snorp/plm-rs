![License](http://img.shields.io/badge/license-MIT-lightgrey.svg)
[![Crates.io](https://img.shields.io/crates/v/plm-rs.svg)](https://crates.io/crates/plm)
[![Doc.rs](https://docs.rs/plm-rs/badge.svg)](https://docs.rs/crate/plm)

# plm

`plm` is a crate for interacting with INSTEON&reg; home automation devices via a PowerLinc Modem. Although most of the public API is `async`, plm-rs is intended to be runtime-agnostic to allow maximum flexibility for apps.

There is a command line app included as a demo. Install it with:

`cargo install plm`

Turn on the device with address `22.33.44` via the modem on `/dev/ttyUSB0`

`plm -d /dev/ttyUSB0 device on 22.33.44`

*Copyright &copy; 2020 James Willcox <snorp@snorp.net>*

