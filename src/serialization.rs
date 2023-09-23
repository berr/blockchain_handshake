use anyhow::Result;
use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[async_trait]
pub trait SerializePayload {
    async fn serialize<W: AsyncWrite + Unpin + Send>(&self, writer: &mut W) -> Result<()>;
}

#[async_trait]
pub trait DeserializePayload: Sized {
    async fn deserialize<R: AsyncRead + Unpin + Send>(reader: &mut R) -> Result<Self>;
}

pub struct CompactSizeUint(pub u64);

#[async_trait]
impl SerializePayload for CompactSizeUint {
    async fn serialize<W: AsyncWrite + Unpin + Send>(&self, writer: &mut W) -> Result<()> {
        let value = self.0;
        match value {
            0..=0xFC => writer.write_u8(value as u8).await?,
            0xFD..=0xFFFF => {
                writer.write_u8(0xFD).await?;
                writer.write_u16(value as u16).await?
            }
            0x10000..=0xFFFFFFFF => {
                writer.write_u8(0xFE).await?;
                writer.write_u32(value as u32).await?
            }
            0x100000000..=0xFFFFFFFFFFFFFFFF => {
                writer.write_u8(0xFD).await?;
                writer.write_u64(value).await?
            }
        }

        Ok(())
    }
}

#[async_trait]
impl DeserializePayload for CompactSizeUint {
    async fn deserialize<R: AsyncRead + Unpin + Send>(reader: &mut R) -> Result<Self> {
        let first_byte = reader.read_u8().await?;

        let value = match first_byte {
            0..=0xFC => first_byte as u64,
            0xFD => reader.read_u16().await? as u64,
            0xFE => reader.read_u32().await? as u64,
            0xFF => reader.read_u64().await?,
        };

        Ok(Self(value))
    }
}
