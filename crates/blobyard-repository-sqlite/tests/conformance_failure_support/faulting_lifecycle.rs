use super::Faulting;
use blobyard_contract::LifecycleRepository;

impl<T: LifecycleRepository> LifecycleRepository for Faulting<'_, T> {
    blobyard_testkit::impl_faulting_lifecycle_repository!();
}
