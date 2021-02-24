use serde::__private::Formatter;
use sled::IVec;
use std::error::Error;
use std::fmt::{Debug, Display};
use std::marker::PhantomData;
use thiserror::Error;

pub trait DatabaseEncode {
    fn to_bytes(&self) -> IVec;
}

pub trait DatabaseDecode: Sized {
    fn from_bytes(data: &IVec) -> Result<Self, DecodingError>;
}

pub trait Database {
    fn insert_entry<K, V>(&self, key: &K, value: &V) -> Result<Option<V>, DatabaseError>
    where
        K: DatabaseEncode + DatabaseDecode,
        V: DatabaseEncode + DatabaseDecode;

    fn get_value<K, V>(&self, key: &K) -> Result<Option<V>, DatabaseError>
    where
        K: DatabaseEncode,
        V: DatabaseEncode + DatabaseDecode;

    fn remove_entry<K, V>(&self, key: &K) -> Result<Option<V>, DatabaseError>
    where
        K: DatabaseEncode,
        V: DatabaseEncode + DatabaseDecode;
}

pub trait PrefixSearchable: Database {
    type Err: DbErr + 'static;
    type Iter: Iterator<Item = Result<(IVec, IVec), Self::Err>>;

    fn find_by_prefix<KP, K, V>(&self, key_prefix: &KP) -> DbIter<Self::Iter, Self::Err, K, V>
    where
        KP: DatabaseEncode,
        K: DatabaseEncode + DatabaseDecode,
        V: DatabaseEncode + DatabaseDecode;
}

pub struct DbIter<Iter, Err, K, V>
where
    Iter: Iterator<Item = Result<(IVec, IVec), Err>>,
    Err: DbErr,
    K: DatabaseEncode + DatabaseDecode,
    V: DatabaseEncode + DatabaseDecode,
{
    iter: Iter,
    _pd: PhantomData<(K, V)>,
}

impl Database for sled::transaction::TransactionalTree {
    fn insert_entry<K, V>(&self, key: &K, value: &V) -> Result<Option<V>, DatabaseError>
    where
        K: DatabaseEncode + DatabaseDecode,
        V: DatabaseEncode + DatabaseDecode,
    {
        match self.insert(key.to_bytes(), value.to_bytes())? {
            Some(old_val_bytes) => Ok(Some(V::from_bytes(&old_val_bytes)?)),
            None => Ok(None),
        }
    }

    fn get_value<K, V>(&self, key: &K) -> Result<Option<V>, DatabaseError>
    where
        K: DatabaseEncode,
        V: DatabaseEncode + DatabaseDecode,
    {
        let key_bytes = key.to_bytes();
        let value_bytes = match self.get(&key_bytes)? {
            Some(value) => value,
            None => return Ok(None),
        };

        Ok(Some(V::from_bytes(&value_bytes)?))
    }

    fn remove_entry<K, V>(&self, key: &K) -> Result<Option<V>, DatabaseError>
    where
        K: DatabaseEncode,
        V: DatabaseEncode + DatabaseDecode,
    {
        let key_bytes = key.to_bytes();
        let value_bytes = match self.remove(&key_bytes)? {
            Some(value) => value,
            None => return Ok(None),
        };

        Ok(Some(V::from_bytes(&value_bytes)?))
    }
}

impl Database for sled::Tree {
    fn insert_entry<K, V>(&self, key: &K, value: &V) -> Result<Option<V>, DatabaseError>
    where
        K: DatabaseEncode + DatabaseDecode,
        V: DatabaseEncode + DatabaseDecode,
    {
        match self.insert(key.to_bytes(), value.to_bytes())? {
            Some(old_val_bytes) => Ok(Some(V::from_bytes(&old_val_bytes)?)),
            None => Ok(None),
        }
    }

    fn get_value<K, V>(&self, key: &K) -> Result<Option<V>, DatabaseError>
    where
        K: DatabaseEncode,
        V: DatabaseEncode + DatabaseDecode,
    {
        let key_bytes = key.to_bytes();
        let value_bytes = match self.get(&key_bytes)? {
            Some(value) => value,
            None => return Ok(None),
        };

        Ok(Some(V::from_bytes(&value_bytes)?))
    }

    fn remove_entry<K, V>(&self, key: &K) -> Result<Option<V>, DatabaseError>
    where
        K: DatabaseEncode,
        V: DatabaseEncode + DatabaseDecode,
    {
        let key_bytes = key.to_bytes();
        let value_bytes = match self.remove(&key_bytes)? {
            Some(value) => value,
            None => return Ok(None),
        };

        Ok(Some(V::from_bytes(&value_bytes)?))
    }
}

impl PrefixSearchable for sled::Tree {
    type Err = sled::Error;
    type Iter = sled::Iter;

    fn find_by_prefix<KP, K, V>(&self, key_prefix: &KP) -> DbIter<Self::Iter, Self::Err, K, V>
    where
        KP: DatabaseEncode,
        V: DatabaseEncode + DatabaseDecode,
        K: DatabaseEncode + DatabaseDecode,
    {
        let prefix_bytes = key_prefix.to_bytes();
        DbIter {
            iter: self.scan_prefix(&prefix_bytes),
            _pd: Default::default(),
        }
    }
}

impl<Iter, Err, K, V> Iterator for DbIter<Iter, Err, K, V>
where
    Iter: Iterator<Item = Result<(IVec, IVec), Err>>,
    Err: DbErr + 'static,
    K: DatabaseEncode + DatabaseDecode,
    V: DatabaseEncode + DatabaseDecode,
{
    type Item = Result<(K, V), DatabaseError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next()? {
            Ok((key_bytes, value_bytes)) => {
                let key = match K::from_bytes(&key_bytes) {
                    Ok(key) => key,
                    Err(e) => return Some(Err(e.into())),
                };
                let value = match V::from_bytes(&value_bytes) {
                    Ok(value) => value,
                    Err(e) => return Some(Err(e.into())),
                };
                Some(Ok((key, value)))
            }
            Err(e) => Some(Err(e.into())),
        }
    }
}

impl DatabaseEncode for () {
    fn to_bytes(&self) -> IVec {
        vec![].into()
    }
}

impl DatabaseDecode for () {
    fn from_bytes(data: &IVec) -> Result<Self, DecodingError> {
        if data.is_empty() {
            Ok(())
        } else {
            Err(DecodingError("Expected zero bytes for empty tuple".into()))
        }
    }
}

#[derive(Debug, Error)]
pub struct DecodingError(pub Box<dyn Error>);

impl Display for DecodingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("Underlying Database Error: {0}")]
    DbError(Box<dyn Error>),
    #[error("Decoding error: {0}")]
    DecodingError(DecodingError),
}

pub trait DbErr: Error {}
impl DbErr for sled::transaction::UnabortableTransactionError {}
impl DbErr for sled::Error {}

impl From<DecodingError> for DatabaseError {
    fn from(e: DecodingError) -> Self {
        DatabaseError::DecodingError(e)
    }
}

impl<E: DbErr + 'static> From<E> for DatabaseError {
    fn from(e: E) -> Self {
        DatabaseError::DbError(Box::new(e))
    }
}
