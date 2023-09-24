use crate::message::Port;
use anyhow::{anyhow, bail, Result};
use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Cursor, Read};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub trait SerializePayload {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()>;
}

pub trait DeserializePayload: Sized {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self>;
}

pub struct CompactSizeUint(pub u64);

impl SerializePayload for CompactSizeUint {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        let value = self.0;
        match value {
            0..=0xFC => buffer.write_u8(value as u8)?,
            0xFD..=0xFFFF => {
                buffer.write_u8(0xFD)?;
                buffer.write_u16::<LittleEndian>(value as u16)?
            }
            0x10000..=0xFFFFFFFF => {
                buffer.write_u8(0xFE)?;
                buffer.write_u32::<LittleEndian>(value as u32)?
            }
            0x100000000..=0xFFFFFFFFFFFFFFFF => {
                buffer.write_u8(0xFD)?;
                buffer.write_u64::<LittleEndian>(value)?
            }
        }

        Ok(())
    }
}

impl DeserializePayload for CompactSizeUint {
    fn deserialize(reader: &mut Cursor<&[u8]>) -> Result<Self> {
        let first_byte = reader.read_u8()?;

        let value = match first_byte {
            0..=0xFC => first_byte as u64,
            0xFD => reader.read_u16::<LittleEndian>()? as u64,
            0xFE => reader.read_u32::<LittleEndian>()? as u64,
            0xFF => reader.read_u64::<LittleEndian>()?,
        };

        Ok(Self(value))
    }
}

impl SerializePayload for u8 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        buffer.write_u8(*self)?;
        Ok(())
    }
}

impl DeserializePayload for u8 {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(buffer.read_u8()?)
    }
}

impl SerializePayload for u16 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        buffer.write_u16::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl DeserializePayload for u16 {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(buffer.read_u16::<LittleEndian>()?)
    }
}

impl SerializePayload for u32 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        buffer.write_u32::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl DeserializePayload for u32 {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(buffer.read_u32::<LittleEndian>()?)
    }
}

impl SerializePayload for u64 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        buffer.write_u64::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl DeserializePayload for u64 {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(buffer.read_u64::<LittleEndian>()?)
    }
}

impl SerializePayload for i32 {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        buffer.write_i32::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl DeserializePayload for i32 {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(buffer.read_i32::<LittleEndian>()?)
    }
}

impl SerializePayload for bool {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        buffer.write_u8(*self as u8)?; // Guaranteed to be either 0x0 or 0x1
        Ok(())
    }
}

impl DeserializePayload for bool {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        let byte = buffer.read_u8()?;
        let as_bool = match byte {
            0 => false,
            1 => true,
            _ => bail!("Boolean value must be zero or one. Received {byte}"),
        };

        Ok(as_bool)
    }
}

impl SerializePayload for Port {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        buffer.write_u16::<BigEndian>(self.0)?;
        Ok(())
    }
}

impl DeserializePayload for Port {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(Port(buffer.read_u16::<BigEndian>()?))
    }
}

impl SerializePayload for IpAddr {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        let as_ipv6_octets = match self {
            IpAddr::V4(ip) => {
                let mut extended = ip.to_ipv6_compatible().octets();
                extended[11] = 0xFF;
                extended[10] = 0xFF;
                extended
            }
            IpAddr::V6(ip) => ip.octets(),
        };

        buffer.extend(&as_ipv6_octets);

        Ok(())
    }
}

impl DeserializePayload for IpAddr {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        let mut octects = [0u8; 16];
        buffer.read_exact(&mut octects)?;

        let ip = if &octects[0..12] == &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF] {
            let ipv4 = &octects[12..16];
            Ipv4Addr::new(ipv4[0], ipv4[1], ipv4[2], ipv4[3]).into()
        } else {
            Ipv6Addr::from(octects).into()
        };

        Ok(ip)
    }
}

impl SerializePayload for SystemTime {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        let seconds_from_epoch = self.duration_since(UNIX_EPOCH)?.as_secs();
        seconds_from_epoch.serialize(buffer)
    }
}

impl DeserializePayload for SystemTime {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        let seconds_from_epoch = u64::deserialize(buffer)?;
        let time = UNIX_EPOCH
            .checked_add(Duration::from_secs(seconds_from_epoch))
            .ok_or_else(|| anyhow!("Couldn't deserialize timestamp: {seconds_from_epoch}"))?;
        Ok(time)
    }
}

impl SerializePayload for String {
    fn serialize(&self, buffer: &mut Vec<u8>) -> Result<()> {
        let size = CompactSizeUint(self.len() as u64);
        size.serialize(buffer)?;
        buffer.extend_from_slice(self.as_bytes());
        Ok(())
    }
}

impl DeserializePayload for String {
    fn deserialize(buffer: &mut Cursor<&[u8]>) -> Result<Self> {
        let size = CompactSizeUint::deserialize(buffer)?;
        let mut string_buffer: Vec<u8> = std::iter::repeat(0u8).take(size.0 as usize).collect();

        buffer.read_exact(&mut string_buffer)?;

        Ok(String::from_utf8(string_buffer)?)
    }
}

#[cfg(test)]
mod test {
    use crate::message::Port;
    use crate::serialization::{DeserializePayload, SerializePayload};
    use std::io::Cursor;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_serde_u8() {
        let original_value: u8 = rand::random();
        let mut buffer = vec![];
        original_value.serialize(&mut buffer).unwrap();
        let deserialized_value = u8::deserialize(&mut Cursor::new(&buffer)).unwrap();
        assert_eq!(original_value, deserialized_value);
    }

    #[test]
    fn test_serde_u16() {
        let original_value: u16 = rand::random();
        let mut buffer = vec![];
        original_value.serialize(&mut buffer).unwrap();
        let deserialized_value = u16::deserialize(&mut Cursor::new(&buffer)).unwrap();
        assert_eq!(original_value, deserialized_value);
    }

    #[test]
    fn test_serde_u32() {
        let original_value: u32 = rand::random();
        let mut buffer = vec![];
        original_value.serialize(&mut buffer).unwrap();
        let deserialized_value = u32::deserialize(&mut Cursor::new(&buffer)).unwrap();
        assert_eq!(original_value, deserialized_value);
    }

    #[test]
    fn test_serde_u64() {
        let original_value: u64 = rand::random();
        let mut buffer = vec![];
        original_value.serialize(&mut buffer).unwrap();
        let deserialized_value = u64::deserialize(&mut Cursor::new(&buffer)).unwrap();
        assert_eq!(original_value, deserialized_value);
    }

    #[test]
    fn test_serde_i32() {
        let original_value: i32 = rand::random();
        let mut buffer = vec![];
        original_value.serialize(&mut buffer).unwrap();
        let deserialized_value = i32::deserialize(&mut Cursor::new(&buffer)).unwrap();
        assert_eq!(original_value, deserialized_value);
    }

    #[test]
    fn test_serde_bool() {
        for original_value in [false, true] {
            let mut buffer = vec![];
            original_value.serialize(&mut buffer).unwrap();
            let deserialized_value = bool::deserialize(&mut Cursor::new(&buffer)).unwrap();
            assert_eq!(original_value, deserialized_value);
        }
    }

    #[test]
    fn test_deserialize_invalid_bool() {
        let buffer = [0x02u8];
        let error_message = bool::deserialize(&mut Cursor::new(&buffer))
            .unwrap_err()
            .to_string();

        assert!(
            error_message.contains("Boolean value must be zero or one. Received 2"),
            "{error_message}"
        );
    }

    #[test]
    fn test_serde_port() {
        let original_value: Port = Port(rand::random());
        let mut buffer = vec![];
        original_value.serialize(&mut buffer).unwrap();
        let deserialized_value = Port::deserialize(&mut Cursor::new(&buffer)).unwrap();
        assert_eq!(original_value, deserialized_value);
    }

    #[test]
    fn test_serde_ipv4() {
        let original_value: IpAddr = Ipv4Addr::new(
            rand::random(),
            rand::random(),
            rand::random(),
            rand::random(),
        )
        .into();

        let mut buffer = vec![];
        original_value.serialize(&mut buffer).unwrap();
        let deserialized_value = IpAddr::deserialize(&mut Cursor::new(&buffer)).unwrap();
        assert_eq!(original_value, deserialized_value);
    }

    #[test]
    fn test_serde_ipv6() {
        let original_value: IpAddr = Ipv6Addr::new(
            rand::random(),
            rand::random(),
            rand::random(),
            rand::random(),
            rand::random(),
            rand::random(),
            rand::random(),
            rand::random(),
        )
        .into();

        let mut buffer = vec![];
        original_value.serialize(&mut buffer).unwrap();
        let deserialized_value = IpAddr::deserialize(&mut Cursor::new(&buffer)).unwrap();
        assert_eq!(original_value, deserialized_value);
    }

    #[test]
    fn test_serde_system_time() {
        let original_value = SystemTime::now();
        let mut buffer = vec![];
        original_value.serialize(&mut buffer).unwrap();
        let deserialized_value = SystemTime::deserialize(&mut Cursor::new(&buffer)).unwrap();

        let original_delta = original_value.duration_since(UNIX_EPOCH).unwrap();
        let deserialized_delta = deserialized_value.duration_since(UNIX_EPOCH).unwrap();

        assert_eq!(original_delta.as_secs(), deserialized_delta.as_secs());
    }

    #[test]
    fn test_serde_empty_string() {
        let original_value = "".to_string();
        let mut buffer = vec![];
        original_value.serialize(&mut buffer).unwrap();
        let deserialized_value = String::deserialize(&mut Cursor::new(&buffer)).unwrap();

        assert_eq!(original_value, deserialized_value);
    }

    #[test]
    fn test_serde_non_empty_string() {
        let original_value = "Hello".to_string();
        let mut buffer = vec![];
        original_value.serialize(&mut buffer).unwrap();
        let deserialized_value = String::deserialize(&mut Cursor::new(&buffer)).unwrap();

        assert_eq!(original_value, deserialized_value);
    }
}
