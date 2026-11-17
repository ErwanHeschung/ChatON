package io.github.erwanheschung.chaton

import android.Manifest
import android.app.Activity
import android.content.Context
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Build
import android.webkit.WebView
import app.tauri.PermissionState
import app.tauri.annotation.Command
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

/**
 * Voice (LiveKit, in core): WebRTC's Java audio needs the JVM and the app context
 * before any sound, the microphone needs a runtime permission, and a call wants
 * the phone in call mode. Called from Rust (`src/voice.rs`).
 */
@TauriPlugin(permissions = [Permission(strings = [Manifest.permission.RECORD_AUDIO], alias = "microphone")])
class VoicePlugin(private val activity: Activity) : Plugin(activity) {
  private val audio get() = activity.getSystemService(AudioManager::class.java)

  override fun load(webView: WebView) {
    initWebrtc(activity.applicationContext)
  }

  /** Before joining: asks for the microphone once, then call mode. Resolves `{granted}`. */
  @Command
  fun prepare(invoke: Invoke) {
    if (getPermissionState("microphone") == PermissionState.GRANTED) {
      ready(invoke)
    } else {
      requestPermissionForAlias("microphone", invoke, "onMicrophone")
    }
  }

  @PermissionCallback
  private fun onMicrophone(invoke: Invoke) = ready(invoke)

  private fun ready(invoke: Invoke) {
    callMode()
    invoke.resolve(JSObject().put("granted", getPermissionState("microphone") == PermissionState.GRANTED))
  }

  /** After leaving: normal audio again. */
  @Command
  fun release(invoke: Invoke) {
    if (Build.VERSION.SDK_INT >= 31) {
      audio.clearCommunicationDevice()
    } else {
      @Suppress("DEPRECATION")
      audio.isSpeakerphoneOn = false
    }
    audio.mode = AudioManager.MODE_NORMAL
    invoke.resolve()
  }

  // Echo cancellation needs the communication mode. Its default output is the earpiece:
  // a headset when there is one, else the loudspeaker, like a voice chat.
  private fun callMode() {
    audio.mode = AudioManager.MODE_IN_COMMUNICATION
    if (Build.VERSION.SDK_INT >= 31) {
      val devices = audio.availableCommunicationDevices
      val headset = devices.firstOrNull { it.type in HEADSETS }
      val speaker = devices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }
      (headset ?: speaker)?.let { audio.setCommunicationDevice(it) }
    } else {
      val outputs = audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS)
      @Suppress("DEPRECATION")
      audio.isSpeakerphoneOn = outputs.none { it.type in HEADSETS }
    }
  }

  /** `Java_io_github_erwanheschung_chaton_VoicePlugin_initWebrtc` in `src/voice.rs`. */
  private external fun initWebrtc(context: Context)

  companion object {
    private val HEADSETS = setOf(
      AudioDeviceInfo.TYPE_WIRED_HEADSET,
      AudioDeviceInfo.TYPE_WIRED_HEADPHONES,
      AudioDeviceInfo.TYPE_USB_HEADSET,
      AudioDeviceInfo.TYPE_BLUETOOTH_SCO,
      AudioDeviceInfo.TYPE_BLE_HEADSET,
    )
  }
}
