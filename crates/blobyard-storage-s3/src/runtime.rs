use crate::RuntimeBridge;
use blobyard_contract::StorageError;
use std::future::Future;
use std::sync::mpsc;

impl RuntimeBridge {
    pub(crate) fn start() -> Result<Self, StorageError> {
        let (handle_sender, handle_receiver) = mpsc::sync_channel(1);
        let (shutdown_sender, shutdown_receiver) = mpsc::channel();
        let started = std::thread::Builder::new()
            .name("blobyard-s3-runtime".to_owned())
            .spawn(move || runtime_thread(&handle_sender, &shutdown_receiver))
            .map(|_thread| ());
        finish_start(started, &handle_receiver, shutdown_sender)
    }

    const fn from_handle(
        handle: tokio::runtime::Handle,
        shutdown_sender: mpsc::Sender<()>,
    ) -> Self {
        Self {
            handle,
            shutdown: Some(shutdown_sender),
        }
    }

    pub(crate) fn run<F, T>(&self, future: F) -> Result<T, StorageError>
    where
        F: Future<Output = Result<T, StorageError>> + Send + 'static,
        T: Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.handle.spawn(async move {
            let result = future.await;
            let _ignored = sender.send(result);
        });
        receive_result(&receiver)
    }
}

impl Drop for RuntimeBridge {
    fn drop(&mut self) {
        if let Some(sender) = self.shutdown.take() {
            let _ignored = sender.send(());
        }
    }
}

fn runtime_thread(
    handle_sender: &mpsc::SyncSender<Result<tokio::runtime::Handle, StorageError>>,
    shutdown_receiver: &mpsc::Receiver<()>,
) {
    publish_runtime(build_runtime(), handle_sender, shutdown_receiver);
}

fn build_runtime() -> Result<tokio::runtime::Runtime, StorageError> {
    map_runtime_build(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("blobyard-s3-worker")
            .build(),
    )
}

fn map_runtime_build(
    value: std::io::Result<tokio::runtime::Runtime>,
) -> Result<tokio::runtime::Runtime, StorageError> {
    value.map_err(|_error| StorageError::Unavailable)
}

fn publish_runtime(
    runtime: Result<tokio::runtime::Runtime, StorageError>,
    handle_sender: &mpsc::SyncSender<Result<tokio::runtime::Handle, StorageError>>,
    shutdown_receiver: &mpsc::Receiver<()>,
) {
    match runtime {
        Ok(runtime) => {
            if handle_sender.send(Ok(runtime.handle().clone())).is_ok() {
                let _ignored = shutdown_receiver.recv();
            }
        }
        Err(error) => {
            let _ignored = handle_sender.send(Err(error));
        }
    }
}

fn finish_start(
    started: std::io::Result<()>,
    handle_receiver: &mpsc::Receiver<Result<tokio::runtime::Handle, StorageError>>,
    shutdown_sender: mpsc::Sender<()>,
) -> Result<RuntimeBridge, StorageError> {
    started.map_err(|_error| StorageError::Unavailable)?;
    let handle = handle_receiver
        .recv()
        .map_err(|_error| StorageError::Unavailable)??;
    Ok(RuntimeBridge::from_handle(handle, shutdown_sender))
}

fn receive_result<T>(
    receiver: &mpsc::Receiver<Result<T, StorageError>>,
) -> Result<T, StorageError> {
    receiver
        .recv()
        .map_err(|_error| StorageError::Unavailable)?
}

#[cfg(test)]
mod tests {
    use super::{RuntimeBridge, finish_start, map_runtime_build, publish_runtime, receive_result};
    use blobyard_contract::StorageError;
    use std::sync::mpsc;

    #[test]
    fn runtime_start_and_result_channels_fail_closed() {
        let (_handle_sender, handle_receiver) = mpsc::channel();
        let (shutdown_sender, _shutdown_receiver) = mpsc::channel();
        assert_eq!(
            finish_start(
                Err(std::io::Error::other("thread failure")),
                &handle_receiver,
                shutdown_sender,
            )
            .err(),
            Some(StorageError::Unavailable)
        );

        let (handle_sender, handle_receiver) = mpsc::channel();
        drop(handle_sender);
        let (shutdown_sender, _shutdown_receiver) = mpsc::channel();
        assert_eq!(
            finish_start(Ok(()), &handle_receiver, shutdown_sender).err(),
            Some(StorageError::Unavailable)
        );

        let (handle_sender, handle_receiver) = mpsc::channel();
        drop(handle_sender.send(Err(StorageError::Unavailable)));
        let (shutdown_sender, _shutdown_receiver) = mpsc::channel();
        assert_eq!(
            finish_start(Ok(()), &handle_receiver, shutdown_sender).err(),
            Some(StorageError::Unavailable)
        );

        let (result_sender, result_receiver) = mpsc::channel();
        drop(result_sender);
        assert_eq!(
            receive_result::<()>(&result_receiver),
            Err(StorageError::Unavailable)
        );
    }

    #[test]
    fn runtime_build_and_publication_errors_reach_the_caller() {
        assert_eq!(
            map_runtime_build(Err(std::io::Error::other("runtime failure"))).err(),
            Some(StorageError::Unavailable)
        );
        let (handle_sender, handle_receiver) = mpsc::sync_channel(1);
        let (_shutdown_sender, shutdown_receiver) = mpsc::channel();
        publish_runtime(
            Err(StorageError::Unavailable),
            &handle_sender,
            &shutdown_receiver,
        );
        assert_eq!(
            handle_receiver.recv().ok().and_then(Result::err),
            Some(StorageError::Unavailable)
        );
    }

    #[test]
    fn runtime_publication_and_shutdown_tolerate_disconnected_receivers() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .map_err(|_error| StorageError::Unavailable);
        let (handle_sender, handle_receiver) = mpsc::sync_channel(1);
        drop(handle_receiver);
        let (_shutdown_sender, shutdown_receiver) = mpsc::channel();
        publish_runtime(runtime, &handle_sender, &shutdown_receiver);

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .map_err(|_error| StorageError::Unavailable);
        let dropped = runtime.map(|runtime| {
            let (shutdown_sender, shutdown_receiver) = mpsc::channel();
            drop(shutdown_receiver);
            let bridge = RuntimeBridge::from_handle(runtime.handle().clone(), shutdown_sender);
            drop(bridge);

            let (shutdown_sender, _shutdown_receiver) = mpsc::channel();
            let mut bridge = RuntimeBridge::from_handle(runtime.handle().clone(), shutdown_sender);
            bridge.shutdown = None;
            drop(bridge);
        });
        assert_eq!(dropped, Ok(()));
    }
}
