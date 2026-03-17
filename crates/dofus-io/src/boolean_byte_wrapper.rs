use anyhow::{bail, Result};

pub fn set_flag(flag: u8, offset: u8, value: bool) -> Result<u8> {
    if offset >= 8 {
        bail!("offset must be lesser than 8");
    }

    if value {
        Ok(flag | (1 << offset))
    } else {
        Ok(flag & (255 - (1 << offset)))
    }
}

pub fn get_flag(flag: u8, offset: u8) -> Result<bool> {
    if offset >= 8 {
        bail!("offset must be lesser than 8");
    }

    Ok((flag & (1 << offset)) != 0)
}
