#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum EventType {
    APPEND = 0x0_u8, 
    SNAPSHOT = 0x1_u8, 
    #[default]
    NullVal = 0xff_u8, 
}
impl From<u8> for EventType {
    #[inline]
    fn from(v: u8) -> Self {
        match v {
            0x0_u8 => Self::APPEND, 
            0x1_u8 => Self::SNAPSHOT, 
            _ => Self::NullVal,
        }
    }
}
impl From<EventType> for u8 {
    #[inline]
    fn from(v: EventType) -> Self {
        match v {
            EventType::APPEND => 0x0_u8, 
            EventType::SNAPSHOT => 0x1_u8, 
            EventType::NullVal => 0xff_u8,
        }
    }
}
impl core::str::FromStr for EventType {
    type Err = ();

    #[inline]
    fn from_str(v: &str) -> core::result::Result<Self, Self::Err> {
        match v {
            "APPEND" => Ok(Self::APPEND), 
            "SNAPSHOT" => Ok(Self::SNAPSHOT), 
            _ => Ok(Self::NullVal),
        }
    }
}
impl core::fmt::Display for EventType {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::APPEND => write!(f, "APPEND"), 
            Self::SNAPSHOT => write!(f, "SNAPSHOT"), 
            Self::NullVal => write!(f, "NullVal"),
        }
    }
}
