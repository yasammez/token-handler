/// Helper for proper error propagation when substituting environment variables

use std::collections::HashMap;

pub enum Substitutions {
    Ok(HashMap<String, String>),
    Err(Vec<String>),
}

impl Substitutions {
    pub fn new() -> Self {
        Self::Ok(HashMap::new())
    }

    pub fn ok(self, key: String, val: String) -> Self {
        match self {
            Self::Ok(mut m) => {
                m.insert(key, val);
                Self::Ok(m)
            },
            _ => self,
        }
    }

    pub fn err(self, key: String) -> Self {
        match self {
            Self::Err(mut v) => {
                v.push(key);
                Self::Err(v)
            }
            _ => Self::Err(vec![key]),
        }
    }

}