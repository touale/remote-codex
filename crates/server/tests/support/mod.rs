use remote_codex_protocol::{Call, Frame, Request, VERSION, codec};
use serde_json::{Value, json};
use std::path::Path;
use tokio::{
    io::BufReader,
    net::{
        UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
};
pub(crate) type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub(crate) struct Peer {
    pub(crate) reader: codec::Reader<BufReader<OwnedReadHalf>>,
    pub(crate) writer: OwnedWriteHalf,
    pub(crate) cursor: i64,
}
impl Peer {
    pub(crate) async fn attach(
        socket: &Path,
        identity: &str,
        channel: &str,
        after: i64,
    ) -> TestResult<Self> {
        let (read, mut writer) = UnixStream::connect(socket).await?.into_split();
        codec::write(
            &mut writer,
            &Call {
                protocol: VERSION,
                id: uuid::Uuid::new_v4().to_string(),
                profile: "test".into(),
                expected_identity: Some(identity.into()),
                request: Request::AttachExecution {
                    channel: channel.into(),
                    after,
                },
            },
        )
        .await?;
        let mut reader = codec::Reader::new(BufReader::new(read));
        match reader.next::<Frame>().await? {
            Some(Frame::Result(_)) => Ok(Self {
                reader,
                writer,
                cursor: after,
            }),
            Some(Frame::Error(e)) => Err(e.into()),
            _ => Err("attach failed".into()),
        }
    }
    pub(crate) async fn send(&mut self, operation: &str, message: Value) -> TestResult {
        codec::write(
            &mut self.writer,
            &Frame::Execute {
                operation: operation.into(),
                message,
                approval: None,
                permissions: None,
            },
        )
        .await?;
        Ok(())
    }
    pub(crate) async fn call(&mut self, method: &str, params: Value) -> TestResult<Value> {
        self.send(
            &uuid::Uuid::new_v4().to_string(),
            json!({"id":1,"method":method,"params":params}),
        )
        .await?;
        self.response().await
    }
    pub(crate) async fn response(&mut self) -> TestResult<Value> {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                match self.reader.next::<Frame>().await? {
                    Some(Frame::Event { cursor, message }) => {
                        self.cursor = cursor;
                        if message.get("method").is_none() && message["id"] == 1 {
                            if message.get("error").is_some() {
                                return Err(message.to_string().into());
                            }
                            return Ok(message["result"].clone());
                        }
                    }
                    Some(Frame::Error(e)) => return Err(e.into()),
                    None => return Err("executor closed".into()),
                    _ => {}
                }
            }
        })
        .await?
    }
    pub(crate) async fn detach(mut self) -> TestResult<i64> {
        codec::write(&mut self.writer, &Frame::Detach).await?;
        Ok(self.cursor)
    }
}
