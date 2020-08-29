use std::path::PathBuf;

use anyhow::{Context, Result};
use futures::StreamExt;

use structopt::StructOpt;

use prettytable::{cell, format::FormatBuilder, row, table, Table};

use log::debug;

use tokio::net::TcpStream;

use plm::*;

#[derive(StructOpt, Debug)]
#[structopt(name = "plm")]
struct App {
    /// A path to a serial device with an INSTEON modem connected, e.g. /dev/ttyUSB0
    #[structopt(short, long, parse(from_os_str), conflicts_with = "host", required_unless = "host")]
    device: Option<PathBuf>,

    /// A host to connect over TCP
    #[structopt(short, long, conflicts_with = "device", required_unless = "device")]
    host: Option<String>,

    #[structopt(subcommand)]
    command: AppCommand,
}

#[derive(StructOpt, Debug)]
enum AppCommand {
    Modem(ModemCommand),
    Listen,
    Device(DeviceCommand),
}

#[derive(StructOpt, Debug)]
#[structopt(about = "Device commands")]
enum DeviceCommand {
    /// Turn a device on
    On {
        #[structopt(flatten)]
        common: DeviceFlags,

        /// The level to set for dimmable devices. Defaults to 100.
        #[structopt(short, long, default_value = "100")]
        level: u8,

        /// Perform a "fast" operation, which avoids ramping on dimmers.
        #[structopt(short, long)]
        fast: bool,
    },
    /// Turn a device off
    Off {
        #[structopt(flatten)]
        common: DeviceFlags,

        /// Perform a "fast" operation, which avoids ramping on dimmers.
        #[structopt(short, long)]
        fast: bool,
    },
    /// Ping a device
    Ping {
        #[structopt(flatten)]
        common: DeviceFlags,
    },
    /// Cause a device to emit a beep
    Beep {
        #[structopt(flatten)]
        common: DeviceFlags,
    },
    /// Retrieve current device status
    Status {
        #[structopt(flatten)]
        common: DeviceFlags,
    },
    /// Retrieve current device status
    Version {
        #[structopt(flatten)]
        common: DeviceFlags,
    },
}

#[derive(StructOpt, Debug)]
struct DeviceFlags {
    /// Address of the device
    address: Address,
}

#[derive(StructOpt, Debug)]
#[structopt(about = "Modem commands")]
enum ModemCommand {
    Info,
    Links,
    LinkDevice {
        /// The address of the device to link
        address: Option<Address>,

        /// Links the modem as a controller of the linked device
        #[structopt(short, long, conflicts_with = "responder", conflicts_with = "delete")]
        controller: bool,

        /// Links the modem as a responder to the linked device
        #[structopt(short, long, conflicts_with = "controller", conflicts_with = "delete")]
        responder: bool,

        /// Deletes the link from the linked device
        #[structopt(
            short,
            long,
            conflicts_with = "controller",
            conflicts_with = "responder"
        )]
        delete: bool,

        /// The group number to link, defaults to 1
        #[structopt(short, long, default_value = "1")]
        group: u8,
    },
}

fn create_table() -> Table {
    let mut table = Table::new();
    let format = FormatBuilder::new()
        .column_separator(' ')
        .padding(0, 1)
        .build();

    table.set_format(format);
    table
}

macro_rules! ptable {
	($($e:tt), +) => {
		let mut table = table!($($e),+);
		let format = FormatBuilder::new()
			.column_separator(' ')
			.padding(0, 1)
			.build();

		table.set_format(format);
		table.printstd();
    };
}

async fn modem_info(modem: &mut Modem) -> Result<()> {
    let info = modem.get_info().await?;

    ptable!(
        ["Address", info.address],
        ["Category", info.category],
        ["Subcategory", info.sub_category],
        ["Firmware Version", info.firmware_version]
    );
    Ok(())
}

async fn modem_links(modem: &mut Modem) -> Result<()> {
    let links = modem.get_links().await?;

    let mut table = create_table();
    table.set_titles(row![b->"Address", b->"Mode", b->"Group"]);

    for link in links {
        // It's useless to display all of the flags, since every record
        // will have IN_USE and most will have HAS_BEEN_USED
        let mode = if link.flags.contains(AllLinkFlags::IS_CONTROLLER) {
            "Controller"
        } else {
            "Responder"
        };

        table.add_row(row![link.to, mode, link.group]);
    }

    table.printstd();

    Ok(())
}

async fn modem_link(
    modem: &mut Modem,
    address: Option<Address>,
    mode: AllLinkMode,
    group: u8,
) -> anyhow::Result<()> {
    let response = modem.link_device(address, mode, group).await?;

    ptable!(
        ["Address", response.address],
        ["Mode", response.mode],
        ["Group", response.group],
        ["Category", response.category],
        ["Subcategory", response.sub_category],
        ["Firmware Version", response.firmware_version]
    );

    Ok(())
}

async fn message_listen(modem: &mut Modem) -> Result<()> {
    let mut stream = modem.listen().await?;

    while let Some(message) = stream.next().await {
        println!("{:02x?}", message);
    }

    Ok(())
}

// Maps 0 - 100 into 0 - 0xff
fn remap_level(level: u8) -> u8 {
    ((level as f32 / 100f32) * 255f32) as u8
}

async fn handle_device_command(modem: &mut Modem, command: DeviceCommand) -> Result<()> {
    match command {
        DeviceCommand::On {
            common,
            level,
            fast,
        } => {
            modem
                .send_message(
                    (
                        common.address,
                        if fast { Command::OnFast } else { Command::On },
                        Command::Other(remap_level(level)),
                    )
                        .into(),
                )
                .await?;
        }
        DeviceCommand::Off { common, fast } => {
            modem
                .send_message(
                    (
                        common.address,
                        if fast { Command::OffFast } else { Command::Off },
                    )
                        .into(),
                )
                .await?;
        }
        DeviceCommand::Ping { common } => {
            modem
                .send_message((common.address, Command::Ping).into())
                .await?;
        }
        DeviceCommand::Beep { common } => {
            modem
                .send_message((common.address, Command::Beep).into())
                .await?;
        }
        DeviceCommand::Status { common } => {
            let response = modem
                .send_message((common.address, Command::StatusRequest).into())
                .await?;
            ptable!(
                ["CMD1", format!("{:02x?}", response.cmd1)],
                ["CMD2", format!("{:02x?}", response.cmd2)]
            );
        },
        DeviceCommand::Version { common } => {
            println!("{:?}", u8::from(modem
                .send_message((common.address, Command::VersionQuery).into())
                .await?.cmd2));
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    let app = App::from_args();

    debug!("{:#?}", app);

    let mut modem = if let Some(device) = app.device {
        Modem::from_path(device).with_context(|| "Failed to open modem")?
    } else {
        let stream = TcpStream::connect(app.host.unwrap()).await.with_context(|| "Failed to connect")?;
        Modem::new(stream)
    };

    match app.command {
        AppCommand::Modem(ModemCommand::Info) => modem_info(&mut modem).await?,
        AppCommand::Modem(ModemCommand::Links) => modem_links(&mut modem).await?,
        AppCommand::Modem(ModemCommand::LinkDevice {
            address,
            controller,
            responder,
            delete,
            group,
        }) => {
            let mode = if controller {
                AllLinkMode::Controller
            } else if responder {
                AllLinkMode::Responder
            } else if delete {
                AllLinkMode::Delete
            } else {
                AllLinkMode::Auto
            };

            modem_link(&mut modem, address, mode, group).await?
        }
        AppCommand::Listen => message_listen(&mut modem).await?,
        AppCommand::Device(command) => handle_device_command(&mut modem, command).await?,
    }

    Ok(())
}
