package io.github.erwanheschung.chaton

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  override fun onResume() {
    super.onResume()
    visible = true
    Notifier.clear(this) // the app shows the messages now
  }

  override fun onPause() {
    visible = false
    super.onPause()
  }

  companion object {
    /** In the foreground: a push needs no notification. */
    @Volatile var visible = false
  }
}
