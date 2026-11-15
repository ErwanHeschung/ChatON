package io.github.erwanheschung.chaton

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat

/** One generic notification, replaced by each new push (same id). */
object Notifier {
  private const val CHANNEL = "messages"
  private const val ID = 1

  fun show(context: Context) {
    if (Build.VERSION.SDK_INT >= 26) {
      val channel = NotificationChannel(
        CHANNEL, context.getString(R.string.channel_messages), NotificationManager.IMPORTANCE_HIGH
      )
      context.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }
    val open = PendingIntent.getActivity(
      context, 0,
      Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    )
    val notification = NotificationCompat.Builder(context, CHANNEL)
      .setSmallIcon(android.R.drawable.stat_notify_chat)
      .setContentTitle(context.getString(R.string.app_name))
      .setContentText(context.getString(R.string.new_messages))
      .setContentIntent(open)
      .setAutoCancel(true)
      .setPriority(NotificationCompat.PRIORITY_HIGH)
      .build()
    try {
      NotificationManagerCompat.from(context).notify(ID, notification)
    } catch (_: SecurityException) {
      // POST_NOTIFICATIONS refused: nothing to show.
    }
  }

  fun clear(context: Context) = NotificationManagerCompat.from(context).cancel(ID)
}
