use std::sync::Arc;

/// Native WhatsApp transport backed by `whatsapp-rust`.
///
/// This is the low-level adapter owned by the `whatsapp` abstraction layer.
/// Command and application code must not depend directly on `whatsapp-rust`.
///
/// The actual session construction and lifecycle will be introduced in the
/// native authentication step. For now this type establishes the backend
/// boundary without changing the existing OpenWA runtime.
pub struct NativeClient {
  client: Arc<whatsapp_rust::Client>,
}

impl NativeClient {
  /// Wrap an already-built `whatsapp-rust` client.
  ///
  /// Construction of the underlying client belongs to the native session
  /// management layer and is intentionally not part of this foundation step.
  pub fn from_client(client: Arc<whatsapp_rust::Client>) -> Self {
    Self { client }
  }

  /// Return the underlying `whatsapp-rust` client.
  ///
  /// Kept inside the WhatsApp abstraction boundary so application services
  /// do not need to know about the concrete native implementation.
  pub(crate) fn client(&self) -> &Arc<whatsapp_rust::Client> {
    &self.client
  }
}