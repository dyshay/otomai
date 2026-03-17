/// Server status values
pub mod server_status {
    pub const OFFLINE: u8 = 0;
    pub const STARTING: u8 = 1;
    pub const ONLINE: u8 = 2;
    pub const NOJOIN: u8 = 3;
    pub const SAVING: u8 = 4;
    pub const STOPING: u8 = 5;
    pub const FULL: u8 = 6;
}

/// Server completion values
pub mod server_completion {
    pub const COMPLETION_RECOMANDED: u8 = 0;
    pub const COMPLETION_AVERAGE: u8 = 1;
    pub const COMPLETION_HIGH: u8 = 2;
    pub const COMPLETION_COMING_SOON: u8 = 3;
    pub const COMPLETION_FULL: u8 = 4;
}

/// Build type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BuildType {
    Release = 0,
    Beta = 1,
    Alpha = 2,
    Testing = 3,
    Internal = 4,
    Debug = 5,
    Draft = 6,
}
