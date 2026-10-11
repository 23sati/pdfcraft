use crate::util::hash128;
use hayro_syntax::object::{Array, Dict, MaybeRef, Name, Null, ObjRef, Object, Stream};
use kurbo::Affine;
use std::any::Any;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::{Arc, Mutex};

type CacheMap = HashMap<u128, Option<Box<dyn Any + Send + Sync>>>;
#[derive(Clone)]
pub(crate) struct Cache(Arc<Mutex<CacheMap>>);
impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

impl Cache {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())))
    }

    pub(crate) fn get_or_insert_with<T: Clone + Send + Sync + 'static>(
        &self,
        id: u128,
        f: impl FnOnce() -> Option<T>,
    ) -> Option<T> {
        let mut locked = self.0.lock().unwrap();

        // We can't use `get_or_insert_with` here, because if the closure makes another access to the
        // cache, we end up with a deadlock.
        match locked.entry(id) {
            Entry::Occupied(o) => o
                .get()
                .as_ref()
                .and_then(|val| val.downcast_ref::<T>().cloned()),
            Entry::Vacant(_) => {
                drop(locked);
                let val = f();
                self.0.lock().unwrap().insert(
                    id,
                    val.clone()
                        .map(|val| Box::new(val) as Box<dyn Any + Send + Sync>),
                );

                val
            }
        }
    }
}

/// PdfCraft patch: a byte-budgeted cache of decoded images, keyed by stream and decode
/// target size. Image-heavy pages decode thousands of images; without this every tile and
/// every scale decodes them all again. Entries hold owned pixels, so a hit never changes
/// them; when an insert would pass the budget the whole map is dropped (a fresh epoch),
/// which only costs re-decoding. The budget caps memory per interpreter cache.
#[derive(Clone)]
pub(crate) struct DecodedImageCache(Arc<Mutex<DecodedImageCacheInner>>);

#[derive(Default)]
struct DecodedImageCacheInner {
    map: HashMap<(u128, u32, u32), Option<Box<dyn Any + Send + Sync>>>,
    bytes: usize,
}

impl DecodedImageCache {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(DecodedImageCacheInner::default())))
    }

    pub(crate) fn get_or_insert_with<T: Clone + Send + Sync + 'static>(
        &self,
        key: (u128, u32, u32),
        budget: usize,
        size_of: impl FnOnce(&T) -> usize,
        f: impl FnOnce() -> Option<T>,
    ) -> Option<T> {
        // Fast path: a hit clones the pixels without decoding.
        if let Some(hit) = self
            .0
            .lock()
            .unwrap()
            .map
            .get(&key)
            .and_then(|val| val.as_ref())
            .and_then(|val| val.downcast_ref::<T>().cloned())
        {
            return Some(hit);
        }
        // Miss: decode without holding the lock (decoding must never deadlock the cache).
        let val = f();
        let mut locked = self.0.lock().unwrap();
        // Another thread may have decoded it meanwhile: keep the first copy.
        if let Some(existing) = locked.map.get(&key)
            && let Some(existing) = existing.as_ref().and_then(|val| val.downcast_ref::<T>())
        {
            return Some(existing.clone());
        }
        let size = match &val {
            Some(v) => size_of(v),
            None => 0,
        };
        if locked.bytes.saturating_add(size) > budget {
            locked.map.clear();
            locked.bytes = 0;
        }
        locked.bytes = locked.bytes.saturating_add(size);
        locked.map.insert(
            key,
            val.clone()
                .map(|val| Box::new(val) as Box<dyn Any + Send + Sync>),
        );
        val
    }
}

/// A trait for objects that can generate a unique cache key.
pub trait CacheKey {
    /// Returns the cache key for this object.
    fn cache_key(&self) -> u128;
}

impl<T: CacheKey, U: CacheKey> CacheKey for (T, U) {
    fn cache_key(&self) -> u128 {
        hash128(&(self.0.cache_key(), self.1.cache_key()))
    }
}

impl CacheKey for Dict<'_> {
    fn cache_key(&self) -> u128 {
        hash128(self.data())
    }
}

impl CacheKey for Stream<'_> {
    fn cache_key(&self) -> u128 {
        self.dict().cache_key()
    }
}

impl CacheKey for Null {
    fn cache_key(&self) -> u128 {
        hash128(self)
    }
}

impl CacheKey for bool {
    fn cache_key(&self) -> u128 {
        hash128(self)
    }
}

impl CacheKey for hayro_syntax::object::Number {
    fn cache_key(&self) -> u128 {
        hash128(&self.as_f64().to_bits())
    }
}

impl CacheKey for hayro_syntax::object::String<'_> {
    fn cache_key(&self) -> u128 {
        hash128(self.as_ref())
    }
}

impl CacheKey for Name<'_> {
    fn cache_key(&self) -> u128 {
        hash128(self)
    }
}

impl CacheKey for Array<'_> {
    fn cache_key(&self) -> u128 {
        hash128(self.data())
    }
}

impl CacheKey for Object<'_> {
    fn cache_key(&self) -> u128 {
        match self {
            Object::Null(n) => n.cache_key(),
            Object::Boolean(b) => b.cache_key(),
            Object::Number(n) => n.cache_key(),
            Object::String(s) => s.cache_key(),
            Object::Name(n) => n.cache_key(),
            Object::Dict(d) => d.cache_key(),
            Object::Array(a) => a.cache_key(),
            Object::Stream(s) => s.cache_key(),
        }
    }
}

impl CacheKey for ObjRef {
    fn cache_key(&self) -> u128 {
        hash128(self)
    }
}

impl<T: CacheKey> CacheKey for MaybeRef<T> {
    fn cache_key(&self) -> u128 {
        match self {
            Self::Ref(r) => r.cache_key(),
            Self::NotRef(o) => o.cache_key(),
        }
    }
}

impl CacheKey for Affine {
    fn cache_key(&self) -> u128 {
        let c = self.as_coeffs();
        hash128(&[
            c[0].to_bits(),
            c[1].to_bits(),
            c[2].to_bits(),
            c[3].to_bits(),
            c[4].to_bits(),
            c[5].to_bits(),
        ])
    }
}

impl CacheKey for u128 {
    fn cache_key(&self) -> u128 {
        hash128(self)
    }
}
