package io.github.erwanheschung.chaton

import org.unifiedpush.android.connector.FailedReason
import org.unifiedpush.android.connector.PushService
import org.unifiedpush.android.connector.data.PushEndpoint
import org.unifiedpush.android.connector.data.PushMessage

/**
 * Pushes from the instance (spec/federation.md): the content is constant and
 * says nothing. We only show "New messages"; the app fetches and decrypts them
 * when it's opened.
 */
class ChatonPushService : PushService() {
  override fun onNewEndpoint(endpoint: PushEndpoint, instance: String) = Endpoints.save(this, endpoint)

  override fun onMessage(message: PushMessage, instance: String) {
    if (!MainActivity.visible && !Quiet.on(this)) Notifier.show(this)
  }

  override fun onRegistrationFailed(reason: FailedReason, instance: String) = Endpoints.fail(reason.name)

  override fun onUnregistered(instance: String) = Endpoints.clear(this)
}
