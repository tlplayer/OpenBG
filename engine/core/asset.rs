use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Handle<T> {
    pub(crate) id: u32,
    pub(crate) _marker: std::marker::PhantomData<T>,
}

impl<T> Handle<T> {
    pub fn id(&self) -> u32 { self.id }
}

struct Store<T> {
    // generational indices can be added later; start simple
    items: Vec<Arc<T>>,
}

impl<T> Store<T> {
    fn new() -> Self { Self { items: Vec::new() } }

    fn insert(&mut self, value: T) -> Handle<T> {
        let id = self.items.len() as u32;
        self.items.push(Arc::new(value));
        Handle { id, _marker: std::marker::PhantomData }
    }

    fn get(&self, h: Handle<T>) -> Option<Arc<T>> {
        self.items.get(h.id as usize).cloned()
    }
}

pub struct Assets {
    // TypeId -> boxed Store<T>
    inner: RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl Assets {
    pub fn new() -> Self {
        Self { inner: RwLock::new(HashMap::new()) }
    }

    fn store_mut<T: 'static + Send + Sync>(&self) -> std::sync::RwLockWriteGuard<'_, Store<T>> {
        let mut map = self.inner.write().unwrap();
        let entry = map.entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(Store::<T>::new()));
        // downcast is safe by construction
        std::sync::RwLockWriteGuard::map(map, |_| {
            entry.downcast_mut::<Store<T>>().unwrap()
        })
    }

    fn store<T: 'static + Send + Sync>(&self) -> std::sync::RwLockReadGuard<'_, Store<T>> {
        let map = self.inner.read().unwrap();
        std::sync::RwLockReadGuard::map(map, |m| {
            m.get(&TypeId::of::<T>())
             .and_then(|b| b.downcast_ref::<Store<T>>())
             .expect("store not initialized")
        })
    }

    pub fn insert<T: 'static + Send + Sync>(&self, value: T) -> Handle<T> {
        self.store_mut::<T>().insert(value)
    }

    pub fn get<T: 'static + Send + Sync>(&self, h: Handle<T>) -> Option<Arc<T>> {
        self.store::<T>().get(h)
    }
}