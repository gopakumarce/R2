use thrift::protocol::{TCompactInputProtocol, TCompactOutputProtocol, TMultiplexedOutputProtocol};
use thrift::protocol::{TCompactInputProtocolFactory, TCompactOutputProtocolFactory};
use thrift::server::{TMultiplexedProcessor, TProcessor, TServer};
use thrift::transport::{ReadHalf, WriteHalf};
use thrift::transport::{TFramedReadTransport, TFramedWriteTransport, TIoChannel, TTcpChannel};
use thrift::transport::{TFramedReadTransportFactory, TFramedWriteTransportFactory};

type ThreadSafeProcessor = Box<dyn TProcessor + Send + Sync>;
pub type ClientInputProtocol = TCompactInputProtocol<TFramedReadTransport<ReadHalf<TTcpChannel>>>;
pub type ClientOutputProtocol = TMultiplexedOutputProtocol<
    TCompactOutputProtocol<TFramedWriteTransport<WriteHalf<TTcpChannel>>>,
>;

/// We implement a "Multiplexed" API server, which means that theres one transport connection
/// at 'addr' that all the clients share, and the clients identify themselves with unique
/// names that help in demultiplexing to the right client. And each client of course will have
/// multiple API calls of their own
pub struct ApiSvr {
    addr: String,
    clients: Vec<(String, ThreadSafeProcessor)>,
}

impl ApiSvr {
    /// A new server listening on address 'addr'
    pub fn new(addr: &str) -> ApiSvr {
        ApiSvr {
            addr: addr.to_string(),
            clients: Vec::new(),
        }
    }

    /// Register a client with 'name' and callback 'processor'. The callback has multiple
    /// APIs bundled inside it, defined by the client's thrift API defenition
    pub fn register(&mut self, name: &str, processor: ThreadSafeProcessor) {
        self.clients.push((name.to_string(), processor));
    }

    /// Run the server, listening on the address waiting for clients to make calls
    pub fn run(&mut self) -> thrift::Result<()> {
        let i_tran = TFramedReadTransportFactory::new();
        let i_prot = TCompactInputProtocolFactory::new();
        let o_tran = TFramedWriteTransportFactory::new();
        let o_prot = TCompactOutputProtocolFactory::new();

        let mut mux = TMultiplexedProcessor::new();
        while let Some(c) = self.clients.pop() {
            mux.register(c.0, c.1, false).unwrap();
        }

        let mut server = TServer::new(i_tran, i_prot, o_tran, o_prot, mux, 1);
        server.listen(&self.addr)
    }
}

/// Used by programs outside R2 to establish a session/connection to the API server in R2
pub fn api_client(
    host_port: &str,
    service: &str,
) -> thrift::Result<(ClientInputProtocol, ClientOutputProtocol)> {
    let mut c = TTcpChannel::new();
    c.open(host_port)?;
    let (i_chan, o_chan) = c.split()?;
    let i_tran = TFramedReadTransport::new(i_chan);
    let o_tran = TFramedWriteTransport::new(o_chan);
    let i_prot = TCompactInputProtocol::new(i_tran);
    let o_prot = TCompactOutputProtocol::new(o_tran);
    let o_prot = TMultiplexedOutputProtocol::new(service, o_prot);
    Ok((i_prot, o_prot))
}
