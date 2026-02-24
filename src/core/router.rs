use tokio::sync::mpsc;

use super::error::Result;
use super::provider::{MessagingProvider, ProviderEvent};
use super::types::Platform;

pub struct MessageRouter {
    providers: Vec<Box<dyn MessagingProvider>>,
    tx: mpsc::UnboundedSender<ProviderEvent>,
    rx: mpsc::UnboundedReceiver<ProviderEvent>,
}

impl MessageRouter {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            providers: Vec::new(),
            tx,
            rx,
        }
    }

    pub fn register_provider(&mut self, provider: Box<dyn MessagingProvider>) {
        self.providers.push(provider);
    }

    pub async fn start_all(&mut self) -> Result<()> {
        for provider in &mut self.providers {
            tracing::info!("Starting provider: {}", provider.name());
            provider.start(self.tx.clone()).await?;
        }
        Ok(())
    }

    pub fn poll_events(&mut self) -> Vec<ProviderEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.rx.try_recv() {
            events.push(event);
        }
        events
    }

    pub fn get_provider(&self, platform: Platform) -> Option<&dyn MessagingProvider> {
        self.providers
            .iter()
            .find(|p| p.platform() == platform)
            .map(|p| p.as_ref())
    }

    pub fn get_provider_mut(&mut self, platform: Platform) -> Option<&mut Box<dyn MessagingProvider>> {
        self.providers
            .iter_mut()
            .find(|p| p.platform() == platform)
    }

    pub async fn stop_all(&mut self) -> Result<()> {
        for provider in &mut self.providers {
            tracing::info!("Stopping provider: {}", provider.name());
            provider.stop().await?;
        }
        Ok(())
    }
}
