use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EntityFootprint {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
