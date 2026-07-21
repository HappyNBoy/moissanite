use std::any::type_name;
use std::cmp::PartialEq;
use std::fmt::Debug;
use std::marker::PhantomData;
use anyhow::{anyhow, bail, Result};
use num_traits::{PrimInt, Signed, Unsigned};
use crate::types::{FunctionType, Global, GlobalType, InstructionKind, Limit, TableType, Value, ValueType};

pub struct BinaryReader<'a> {
    bytes: &'a [u8],
}

impl<'src> BinaryReader<'src> {
    pub const fn new(bytes: &[u8]) -> BinaryReader<'_> {
        BinaryReader { bytes }
    }

    pub const fn len(&self) -> usize {
        self.bytes.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn read<T: Readable>(&mut self) -> Result<T> {
        Readable::read(self)
    }

    pub fn consume(self) -> Result<()> {
        if !self.is_empty() {
            bail!("Extra unexpected data found in scope");
        }
        Ok(())
    }

    pub fn scope_in<T>(&mut self, parser: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let len = self.read::<u32>()? as usize;
        if len > self.bytes.len() {
            bail!("Inner scope goes out of outer scope");
        }
        let (scope, rest) = self.bytes.split_at(len);
        self.bytes = rest;
        let mut scoped_reader = Self::new(scope);
        let out = parser(&mut scoped_reader)?;
        scoped_reader.consume()?;
        Ok(out)
    }

    pub fn skip_bytes(&mut self, n: usize) -> Result<()> {
        if self.len() < n {
            bail!("Expected {n} bytes, found only {}", self.len());
        }
        self.bytes = &self.bytes[n..];
        Ok(())
    }
    
    pub fn skip_scope(&mut self) -> Result<()> {
        let len = self.read::<u32>()? as usize;
        self.skip_bytes(len)
    }
    
    pub fn read_vec<T: Readable>(&mut self) -> Result<VecReader<'_, 'src, T>> {
        let count = self.read::<u32>()? as usize;
        Ok(VecReader {
            reader: self,
            count,
            _phantom: PhantomData,
        })
    }

    pub fn read_const_expr(&mut self) -> Result<Value> {
        let insn_kind = self.read::<InstructionKind>()?;
        let value = match insn_kind {
            InstructionKind::I32Const => Value::I32(self.read()?),
            InstructionKind::I64Const => Value::I64(self.read()?),
            InstructionKind::F32Const => Value::F32(self.read()?),
            InstructionKind::F64Const => Value::F64(self.read()?),
            _ => bail!("Invalid constant expression, found instruction {insn_kind:?}")
        };
        if self.read::<InstructionKind>()? != InstructionKind::EndInstructions {
            bail!("Invalid constant expression, multiple instructions");
        }
        Ok(value)
    }
}

pub trait Readable: Sized {
    fn read(reader: &mut BinaryReader) -> Result<Self>;
}

impl Readable for u8 {
    fn read(reader: &mut BinaryReader) -> Result<u8> {
        if reader.is_empty() {
            bail!("Expected u8, found end of section");
        }
        let out = reader.bytes[0];
        reader.bytes = &reader.bytes[1..];
        Ok(out)
    }
}

#[derive(PartialEq, Copy, Clone, Debug)]
#[repr(transparent)]
pub struct UnencodedU32(pub u32);

impl From<u64> for UnencodedU32 {
    fn from(value: u64) -> Self {
        UnencodedU32(value as u32)
    }
}

impl Readable for UnencodedU32 {
    fn read(reader: &mut BinaryReader) -> Result<UnencodedU32> {
        if reader.len() < size_of::<u32>() {
            bail!("Expected u32, found end of section");
        }
        let (bytes, rest) = reader.bytes.split_at(size_of::<u32>());
        reader.bytes = rest;
        Ok(UnencodedU32(u32::from_le_bytes(bytes.try_into()?)))
    }
}

// LEB128 encoding
fn unsigned_leb128<T: PrimInt + Unsigned>(reader: &mut BinaryReader) -> Result<T> {
    let mut out = T::zero();
    let mut shift = 0;
    let max_shift = size_of::<T>() * 8;

    loop {
        if reader.bytes.is_empty() {
            bail!("Expected {}, found end of section", type_name::<T>());
        }

        // Pull the next byte
        let byte = reader.read::<u8>()?;

        // Extract the 7 payload bits
        let payload = byte & 0x7F;

        // Validation: Ensure the bits won't overflow our target type T
        // We check if the payload has bits set beyond what T can accept
        if shift >= max_shift || (shift + 7 > max_shift && (payload >> (max_shift - shift)) != 0) {
            bail!("Invalid LEB128 encoding encountered while reading {}", type_name::<T>());
        }

        // Shift payload to its correct little-endian position and mix it in
        out = out | (T::from(payload).unwrap() << shift);

        // If MSB (0x80) is clear, this is the last byte
        if (byte & 0x80) == 0 {
            break;
        }
        shift += 7;
    }
    
    Ok(out)
}

fn signed_leb128<T: PrimInt + Signed>(reader: &mut BinaryReader) -> Result<T> {
    let mut out = T::zero();
    let mut shift = 0;
    let max_shift = size_of::<T>() * 8;
    let mut byte: u8;

    loop {
        if reader.bytes.is_empty() {
            bail!("Expected {}, found end of section", type_name::<T>());
        }

        byte = reader.read::<u8>()?;

        let payload = byte & 0x7F;

        // Overflow validation for signed types
        if shift >= max_shift || (shift + 7 > max_shift && (payload >> (max_shift - shift)) != 0) {
            bail!("Invalid LEB128 encoding encountered while reading (overflow) {}", type_name::<T>());
        }

        // Shift payload into place
        out = out | (T::from(payload).unwrap() << shift);
        shift += 7;

        // If MSB (0x80) is clear, this is the last byte
        if (byte & 0x80) == 0 {
            break;
        }
    }

    // Sign extension: Check the 7th bit of the last consumed byte
    if shift < max_shift && (byte & 0x40) != 0 {
        // Create a mask of 1s for all bits above the shifted payload
        // e.g., if shift is 7, mask is 11111111_11111111_11111111_10000000
        let sign_extend_mask = (!T::zero()) << shift;
        out = out | sign_extend_mask;
    }
    
    Ok(out)
}

impl Readable for u32 {
    fn read(reader: &mut BinaryReader) -> Result<u32> {
        unsigned_leb128(reader)
    }
}

impl Readable for u64 {
    fn read(reader: &mut BinaryReader) -> Result<u64> {
        unsigned_leb128(reader)
    }
}

impl Readable for i32 {
    fn read(reader: &mut BinaryReader) -> Result<i32> {
        signed_leb128(reader)
    }
}

impl Readable for i64 {
    fn read(reader: &mut BinaryReader) -> Result<i64> {
        signed_leb128(reader)
    }
}

impl Readable for f32 {
    fn read(reader: &mut BinaryReader) -> Result<f32> {
        if reader.len() < size_of::<f32>() {
            bail!("Expected f32, found end of section");
        }
        let (bytes, rest) = reader.bytes.split_at(size_of::<f32>());
        reader.bytes = rest;
        Ok(f32::from_le_bytes(bytes.try_into()?))
    }
}

impl Readable for f64 {
    fn read(reader: &mut BinaryReader) -> Result<f64> {
        if reader.len() < size_of::<f64>() {
            bail!("Expected f64, found end of section");
        }
        let (bytes, rest) = reader.bytes.split_at(size_of::<f64>());
        reader.bytes = rest;
        Ok(f64::from_le_bytes(bytes.try_into()?))
    }
}

pub struct VecReader<'a, 'src, T: Readable> {
    reader: &'a mut BinaryReader<'src>,
    count: usize,
    _phantom: PhantomData<T>,
}

impl<T: Readable> Iterator for VecReader<'_, '_, T> {
    type Item = Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count > 0 {
            self.count -= 1;
            Some(self.reader.read())
        } else {
            None
        }
    }
}

impl<T: Readable> ExactSizeIterator for VecReader<'_, '_, T> {
    fn len(&self) -> usize {
        self.count
    }
}


impl<T: Readable> Readable for Box<[T]> {
    fn read(reader: &mut BinaryReader) -> Result<Box<[T]>> {
        // let len = reader.read::<u32>()? as usize;
        // (0..len).map(|_| T::read(reader)).collect::<Result<Box<[T]>>>()
        reader.read_vec()?.collect::<Result<Box<[T]>>>()
    }
}

impl Readable for ValueType {
    fn read(reader: &mut BinaryReader) -> Result<ValueType> {
        reader.read::<u8>()?.try_into()
    }
}

impl Readable for FunctionType {
    fn read(reader: &mut BinaryReader) -> Result<FunctionType> {
        Ok(FunctionType {
            _prefix: reader.read()?,
            parameters: reader.read()?,
            results: reader.read()?,
        })
    }
}

impl Readable for bool {
    fn read(reader: &mut BinaryReader) -> Result<bool> {
        Ok(match reader.read::<u8>()? {
            0x00 => false,
            0x01 => true,
            other => bail!("Invalid boolean flag 0x{other:x?}, expected 0x00 or 0x01")
        })
    }
}

impl Readable for Limit {
    fn read(reader: &mut BinaryReader) -> Result<Limit> {
        let flag = reader.read::<bool>()?;
        let min = reader.read::<u32>()?;
        Ok(Limit {
            min,
            max: if flag {
                let max = reader.read::<u32>()?;
                if max < min {
                    bail!("Limit max {max} must not be smaller than its min {min}");
                }
                Some(max)
            } else {
                None
            }
        })
    }
}

impl Readable for TableType {
    fn read(reader: &mut BinaryReader) -> Result<TableType> {
        Ok(TableType {
            _prefix: reader.read()?,
            limit: reader.read()?,
        })
    }
}

impl Readable for GlobalType {
    fn read(reader: &mut BinaryReader) -> Result<Self> {
        Ok(GlobalType {
            v_type: reader.read()?,
            is_mut: reader.read()?,
        })
    }
}

impl Readable for InstructionKind {
    fn read(reader: &mut BinaryReader) -> Result<InstructionKind> {
        let byte = reader.read::<u8>()?;
        InstructionKind::try_from(byte)
            .map_err(|_| anyhow!("Invalid instruction kind: 0x{byte:x?}"))
    }
}

impl Readable for Global {
    fn read(reader: &mut BinaryReader) -> Result<Global> {
        let global_type = reader.read::<GlobalType>()?;
        let value = reader.read_const_expr()?;

        if ValueType::from(value) != global_type.v_type {
            bail!("Global init expression does not match its type");
        }

        Ok(Global {
            mutable: global_type.is_mut,
            value,
        })
    }
}