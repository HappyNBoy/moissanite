mod types;
mod reader;
mod parse;
mod output;

use std::fs;
use std::io::Write;
use anyhow::{bail, Result};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use flate2::Compression;
use flate2::write::GzEncoder;
use crate::output::Output;
use crate::parse::parse_module;
use crate::reader::BinaryReader;
use crate::types::{FunctionType, Global, Limit};

#[derive(clap::Parser)]
struct Args {
    input: String,
}

struct Context {
    args: Args,
    function_types: Box<[FunctionType]>,
    functions: Box<[u32]>,
    tables: Box<[Limit]>,
    memories: Box<[Limit]>,
    globals: Box<[Global]>,
    out: Output,
}

fn dummy_parse() -> Args {
    Args {
        input: "./wasm_dummy.wasm".to_string()
    }
}

fn main() -> Result<()> {
    let mut ctx = Context {
        args: dummy_parse(),
        function_types: Box::new([]),
        functions: Box::new([]),
        tables: Box::new([]),
        memories: Box::new([]),
        globals: Box::new([]),
        out: Output::new(),
    };
    
    let bytes = fs::read(&ctx.args.input)?;
    let mut reader = BinaryReader::new(&bytes);
    parse_module(&mut reader, &mut ctx)?;
    dbg!(ctx.memories);
    // reader.consume()?;
    place_templates(ctx.out)?;
    Ok(())
}

fn encode_json(out: Output) -> impl Iterator<Item = Result<String>> {
    out.functions.into_iter().chain(Some(out.init)).map(|line| {
        let json = serde_json::to_vec(&line)?;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&json)?;
        let compressed = encoder.finish()?;
        let encoded = BASE64_STANDARD.encode(&compressed);
        Ok(encoded)
    })
}

// done via CodeClient's api
fn place_templates(out: Output) -> Result<()> {
    use tungstenite::{connect, Message};

    let templates = encode_json(out);

    const URL: &str = "ws://localhost:31375";

    let (mut socket, _) = connect(URL)?;

    socket.send(Message::Text("scopes write_code".into()))?;
    if socket.read()?.to_text()? != "auth" {
        bail!("Did not receive auth from CodeClient");
    }

    socket.write(Message::Text("place swap".into()))?;
    for line in templates {
        socket.write(Message::Text(format!("place {}", line?).into()))?;
    }
    socket.write(Message::Text("place go".into()))?;
    socket.flush()?;

    if socket.read()?.to_text()? != "place done" {
        bail!("Failed to place templates through CodeClient");
    }
    
    socket.close(None)?;

    Ok(())
}