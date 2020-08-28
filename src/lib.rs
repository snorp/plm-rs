#![recursion_limit = "256"]

//! A crate for interacting with INSTEON™ home automation devices via
//! an attached PowerLinc Modem.
//!
//! # Example
//! ```no_run
//! # use std::str::FromStr;
//! # use plm::{Address, Modem, Message, Command};
//! # use plm::Error;
//! # #[tokio::main]
//! # async fn main() -> Result<(), Error>  {
//! // Use the modem attached to /dev/ttyUSB0 to turn on the switch
//! // with address 11.22.33.
//! let mut modem = Modem::from_path("/dev/ttyUSB0")?;
//! modem.send_message((Address::from_str("11.22.33")?, Command::On).into()).await?;
//! # Ok(())
//! # }
//! ```

mod broker;
mod constants;
mod error;
mod frame;
mod message;
mod modem;

pub use error::*;
pub use message::*;
pub use modem::*;

pub use frame::{Address, AllLinkComplete, AllLinkFlags, AllLinkMode, MessageFlags, ModemInfo};
