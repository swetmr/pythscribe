//! `pyths-lsp` binary — the language server entry point.
//!
//! Reads JSON-RPC messages from stdin, dispatches to the `Server`, writes
//! responses and notifications to stdout.

use std::io::{self, BufReader, Write};

use pyths_lsp::{read_message, write_message, Server};

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    let mut server = Server::new();

    loop {
        let msg = match read_message(&mut reader) {
            Ok(Some(m)) => m,
            Ok(None) => break, // EOF
            Err(e) => {
                eprintln!("pyths-lsp: read error: {}", e);
                break;
            }
        };
        let r = server.handle(&msg);
        if let Some(resp) = r.response {
            write_message(&mut writer, &resp)?;
        }
        for n in r.notifications {
            write_message(&mut writer, &n)?;
        }
        if r.exit {
            break;
        }
    }
    writer.flush()?;
    Ok(())
}
