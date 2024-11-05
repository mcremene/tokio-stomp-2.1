use bytes::{Buf, BytesMut};
use futures::prelude::*;
use futures::sink::SinkExt;
use std::net::ToSocketAddrs;
use tokio::net::TcpStream;
use tokio_util::codec::{Decoder, Encoder, Framed};

use native_tls::TlsConnector as NativeTlsConnector;
use tokio_native_tls::{TlsConnector, TlsStream};

pub type ClientTransport = Framed<TcpStream, ClientCodec>;
pub type ClientTlsTransport = Framed<TlsStream<TcpStream>, ClientCodec>;

use crate::frame;
use crate::{FromServer, Message, Result, ToServer};
use anyhow::{anyhow, bail};

/// Connect to a STOMP server via TCP, including the connection handshake.
/// If successful, returns a tuple of a message stream and a sender,
/// which may be used to receive and send messages respectively.
pub async fn connect(
    address: &str,
    login: Option<String>,
    passcode: Option<String>,
) -> Result<ClientTransport> {
    let addr = address.to_socket_addrs().unwrap().next().unwrap();
    let tcp = TcpStream::connect(&addr).await?;
    let mut transport = ClientCodec.framed(tcp);
    client_handshake(&mut transport, address, login, passcode).await?;
    Ok(transport)
}

pub async fn connect_tls(
    domain: &str,
    address: &str,
    login: Option<String>,
    passcode: Option<String>,
) -> Result<ClientTlsTransport> {
    let addr = address.to_socket_addrs()?.next().unwrap();
    // Set up the TLS connector
    let native_tls_connector = NativeTlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .build()?;
    let tls_connector = TlsConnector::from(native_tls_connector);
    let tcp_stream = TcpStream::connect(&addr).await?;
    // Perform the TLS handshake
    let tls_stream: TlsStream<TcpStream> = tls_connector.connect(domain, tcp_stream).await?;
    let mut transport = ClientCodec.framed(tls_stream);
    client_handshake_tls(&mut transport, address, login, passcode).await?;
    Ok(transport)
}

async fn client_handshake(
    transport: &mut ClientTransport,
    address: &str,
    login: Option<String>,
    passcode: Option<String>,
) -> Result<()> {
    let connect = Message {
        content: ToServer::Connect {
            accept_version: "1.2".into(),
            host: address.to_string(),
            login,
            passcode,
            heartbeat: None,
        },
        extra_headers: vec![],
    };
    // Send the message
    transport.send(connect).await?;
    // Receive reply
    let msg = transport.next().await.transpose()?;
    if let Some(FromServer::Connected { .. }) = msg.as_ref().map(|m| &m.content) {
        Ok(())
    } else {
        Err(anyhow!("unexpected reply: {:?}", msg))
    }
}

async fn client_handshake_tls(
    transport: &mut ClientTlsTransport,
    address: &str,
    login: Option<String>,
    passcode: Option<String>,
) -> Result<()> {
    let connect = Message {
        content: ToServer::Connect {
            accept_version: "1.2".into(),
            host: address.to_string(),
            login,
            passcode,
            heartbeat: None,
        },
        extra_headers: vec![],
    };
    // Send the message
    transport.send(connect).await?;
    // Receive reply
    let msg = transport.next().await.transpose()?;
    if let Some(FromServer::Connected { .. }) = msg.as_ref().map(|m| &m.content) {
        Ok(())
    } else {
        Err(anyhow!("unexpected reply: {:?}", msg))
    }
}

/// Convenience function to build a Subscribe message
pub fn subscribe(dest: impl Into<String>, id: impl Into<String>) -> Message<ToServer> {
    ToServer::Subscribe {
        destination: dest.into(),
        id: id.into(),
        ack: None,
    }
    .into()
}

pub struct ClientCodec;

impl Decoder for ClientCodec {
    type Item = Message<FromServer>;
    type Error = anyhow::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
        let (item, offset) = match frame::parse_frame(src) {
            Ok((remain, frame)) => (
                Message::<FromServer>::from_frame(frame),
                remain.as_ptr() as usize - src.as_ptr() as usize,
            ),
            Err(nom::Err::Incomplete(_)) => return Ok(None),
            Err(e) => bail!("Parse failed: {:?}", e),
        };
        src.advance(offset);
        item.map(Some)
    }
}

impl Encoder<Message<ToServer>> for ClientCodec {
    type Error = anyhow::Error;

    fn encode(
        &mut self,
        item: Message<ToServer>,
        dst: &mut BytesMut,
    ) -> std::result::Result<(), Self::Error> {
        item.to_frame().serialize(dst);
        Ok(())
    }
}
