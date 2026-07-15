#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Side {
    BID = 0x0_u8,
    ASK = 0x1_u8,
    #[default]
    NullVal = 0xff_u8,
}
impl From<u8> for Side {
    #[inline]
    fn from(v: u8) -> Self {
        match v {
            0x0_u8 => Self::BID,
            0x1_u8 => Self::ASK,
            _ => Self::NullVal,
        }
    }
}
impl From<Side> for u8 {
    #[inline]
    fn from(v: Side) -> Self {
        match v {
            Side::BID => 0x0_u8,
            Side::ASK => 0x1_u8,
            Side::NullVal => 0xff_u8,
        }
    }
}
impl core::str::FromStr for Side {
    type Err = ();

    #[inline]
    fn from_str(v: &str) -> core::result::Result<Self, Self::Err> {
        match v {
            "BID" => Ok(Self::BID),
            "ASK" => Ok(Self::ASK),
            _ => Ok(Self::NullVal),
        }
    }
}
impl core::fmt::Display for Side {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BID => write!(f, "BID"),
            Self::ASK => write!(f, "ASK"),
            Self::NullVal => write!(f, "NullVal"),
        }
    }
}
