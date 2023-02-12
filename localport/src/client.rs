use std::sync::Arc;

use liblocalport as lib;

use crate::error::{Error, Result};
use crate::transport::{Receiver, Sender, Transport};

struct TxRx {
    sender: Box<dyn Sender>,
    receiver: Box<dyn Receiver>,
}

pub struct Client {
    hostname: String,
    transport: Box<dyn Transport>,
    //    txrx: Option<TxRx>,
}

impl Client {
    pub fn new(hostname: &str, transport: Box<dyn Transport>) -> Self {
        Self {
            hostname: hostname.to_owned(),
            transport,
            //txrx: None,
        }
    }

    // pub fn new_ws(hostname: &str, server: &str) -> Self {
    //     Client::new(hostname, crate::transport::ws(server))
    // }

    pub async fn connect(&self) -> Result<()> {
        let (mut sender, receiver) = self.transport.connect().await?;
        let (tx, rx) = std::sync::mpsc::channel::<lib::client::Message>();
        let mut send_task = tokio::spawn(async move {
            loop {
                match rx.recv() {
                    Ok(msg) => {
                        println!("SEND MSG");
                        sender.send(&msg);
                    }
                    Err(error) => {
                        println!("error receiving {}", error);
                        return;
                    }
                }
            }
        });
        Ok(())
    }

    // async fn txrx(&mut self) -> Result<&TxRx> {
    //     if self.txrx.is_some() {
    //         return Ok(&self.txrx.unwrap());
    //     }
    //     let txrx = &self.txrx;
    //     match &txrx {
    //         Some(txrx) => Ok(txrx),
    //         None => {
    //             let (sender, receiver) = self.transport.connect().await?;
    //             self.txrx = Some(TxRx { sender, receiver });
    //             return self.txrx().await;
    //         }
    //     }
    // }

    // async fn recv(&mut self) -> Result<lib::server::Message> {
    //     let mut txrx = self.txrx()?;
    //     Ok(txrx.receiver.recv().await?)
    // }
}
