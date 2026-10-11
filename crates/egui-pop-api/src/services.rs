//! Services plugins share, by typed token.
//!
//! ```
//! use egui_pop_api::services::ServiceToken;
//!
//! pub struct Clock { pub ticks: u64 }
//! pub const CLOCK: ServiceToken<Clock> = ServiceToken::new("demo.clock");
//! assert_eq!(CLOCK.id().as_str(), "demo.clock");
//! ```

use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use egui_pop_model::ids::{OwnerId, ServiceId};

/// A service's id and type: `ctx.service(&TOKEN, value)` provides it,
/// `app.service(&TOKEN)` gets it back typed.
pub struct ServiceToken<T: ?Sized> {
    id: ServiceId,
    _type: PhantomData<fn() -> Arc<T>>,
}

impl<T: ?Sized> ServiceToken<T> {
    pub const fn new(id: &'static str) -> Self {
        Self {
            id: ServiceId::from_static(id),
            _type: PhantomData,
        }
    }

    pub fn id(&self) -> &ServiceId {
        &self.id
    }
}

impl<T: ?Sized> fmt::Debug for ServiceToken<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ServiceToken({}: {})",
            self.id,
            std::any::type_name::<T>()
        )
    }
}

#[derive(Default)]
pub(crate) struct Services {
    map: BTreeMap<ServiceId, (OwnerId, Arc<dyn Any + Send + Sync>, &'static str)>,
}

impl fmt::Debug for Services {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.map.iter().map(|(k, (o, _, t))| (k, (o, t))))
            .finish()
    }
}

impl Services {
    pub fn provide<T: Send + Sync + 'static>(
        &mut self,
        owner: &OwnerId,
        token: &ServiceToken<T>,
        value: Arc<T>,
    ) -> Result<(), OwnerId> {
        if let Some((by, _, _)) = self.map.get(&token.id) {
            return Err(by.clone());
        }
        self.map.insert(
            token.id.clone(),
            (owner.clone(), value, std::any::type_name::<T>()),
        );
        Ok(())
    }

    pub fn get<T: Send + Sync + 'static>(&self, token: &ServiceToken<T>) -> Option<Arc<T>> {
        self.map
            .get(&token.id)
            .and_then(|(_, v, _)| v.clone().downcast::<T>().ok())
    }

    pub fn remove_owner(&mut self, owner: &OwnerId) {
        self.map.retain(|_, (o, _, _)| o != owner);
    }

    /// Id, owner and type name of each service.
    pub fn list(&self) -> Vec<(ServiceId, OwnerId, &'static str)> {
        self.map
            .iter()
            .map(|(k, (o, _, t))| (k.clone(), o.clone(), *t))
            .collect()
    }
}
