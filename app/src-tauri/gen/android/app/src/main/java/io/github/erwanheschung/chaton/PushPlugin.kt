package io.github.erwanheschung.chaton

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.unifiedpush.android.connector.UnifiedPush
import org.unifiedpush.android.connector.data.PushEndpoint

@InvokeArg
class QuietArgs {
  var on: Boolean = false
}

@InvokeArg
class RegisterArgs {
  /** The instance's VAPID public key (base64url), from `GET /api/push`. */
  var vapid: String = ""
}

/**
 * UnifiedPush registration, called from Rust (`enable_push`). Resolves with
 * `{endpoint, p256dh, auth}`, which core then registers on the instance.
 */
@TauriPlugin
class PushPlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun register(invoke: Invoke) {
    val args = invoke.parseArgs(RegisterArgs::class.java)
    if (Build.VERSION.SDK_INT >= 33 &&
      activity.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
    ) {
      activity.requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 0)
    }
    UnifiedPush.tryUseCurrentOrDefaultDistributor(activity) { ok ->
      if (!ok) {
        invoke.reject("No UnifiedPush distributor installed", "no_distributor")
      } else {
        Endpoints.await(activity, invoke)
        UnifiedPush.register(activity, vapid = args.vapid)
      }
    }
  }

  /** Do not disturb (spec protocol § 12): pushes stay silent. */
  @Command
  fun quiet(invoke: Invoke) {
    Quiet.set(activity, invoke.parseArgs(QuietArgs::class.java).on)
    invoke.resolve()
  }
}

/** Do not disturb, kept for the push service, which runs without the app. */
object Quiet {
  private const val PREFS = "chaton.quiet"

  fun set(context: Context, on: Boolean) =
    context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean("on", on).apply()

  fun on(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean("on", false)
}

/** The endpoint UnifiedPush gave us, kept across restarts; answers a pending `register`. */
object Endpoints {
  private const val PREFS = "chaton.push"
  private var pending: Invoke? = null

  /** Answers right away with the saved endpoint, else when the distributor gives one. */
  @Synchronized
  fun await(context: Context, invoke: Invoke) {
    val saved = load(context)
    if (saved != null) invoke.resolve(saved) else pending = invoke
  }

  @Synchronized
  fun save(context: Context, endpoint: PushEndpoint) {
    val keys = endpoint.pubKeySet
    if (keys == null) {
      fail("the distributor gave no WebPush keys")
      return
    }
    context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
      .putString("endpoint", endpoint.url)
      .putString("p256dh", keys.pubKey)
      .putString("auth", keys.auth)
      .apply()
    pending?.resolve(load(context))
    pending = null
  }

  @Synchronized
  fun fail(reason: String) {
    pending?.reject(reason, "push_failed")
    pending = null
  }

  fun clear(context: Context) {
    context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().clear().apply()
  }

  private fun load(context: Context): JSObject? {
    val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    val endpoint = prefs.getString("endpoint", null) ?: return null
    return JSObject()
      .put("endpoint", endpoint)
      .put("p256dh", prefs.getString("p256dh", ""))
      .put("auth", prefs.getString("auth", ""))
  }
}
