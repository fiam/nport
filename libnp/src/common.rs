// PorMessage is used internally to synchronize access to ports
// in both the client and server
pub enum PortMessage {
    Data(Vec<u8>),
    Close,
}
