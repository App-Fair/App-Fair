// App Fair's periodic background update check (#7). WorkManager runs doWork() on its own worker
// thread — a Java thread that holds the app classloader, so it can call DayInstaller and the Rust
// native method directly. Rust syncs every enabled catalog and returns how many installed apps now
// have an update; if any, we post a low-priority notification whose tap opens App Fair. Scheduled
// from DayInstaller.scheduleUpdateChecks(). No day-toolkit-material edits.
package org.appfair.app.appfair;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.os.Build;

import androidx.annotation.NonNull;
import androidx.work.Worker;
import androidx.work.WorkerParameters;

public final class UpdateWorker extends Worker {
    static final String CHANNEL_ID = "app_fair_updates";
    static final int NOTIFICATION_ID = 1001;

    /** Rust (exported by the app cdylib): sync every enabled catalog and return the number of
     *  installed apps that now have a newer version, given the installed "pkg\tcode" lines and
     *  the app's files and cache directories (a process WorkManager starts has run no activity
     *  to pass them). */
    static native int nativeCheckUpdates(String installedPackages, String filesDir, String cacheDir);

    public UpdateWorker(@NonNull Context context, @NonNull WorkerParameters params) {
        super(context, params);
    }

    @NonNull
    @Override
    public Result doWork() {
        try {
            // Gather installed packages in Java (this thread has the app classloader), then hand
            // them to Rust to sync + compare against the catalog.
            String installed = DayInstaller.installedPackages();
            Context ctx = getApplicationContext();
            int count = nativeCheckUpdates(installed, ctx.getFilesDir().getAbsolutePath(),
                    ctx.getCacheDir().getAbsolutePath());
            if (count > 0) {
                notifyUpdates(getApplicationContext(), count);
            }
            return Result.success();
        } catch (Throwable t) {
            // A failed check simply waits for the next cycle; never crash the worker.
            return Result.success();
        }
    }

    private static void notifyUpdates(Context ctx, int count) {
        NotificationManager nm =
                (NotificationManager) ctx.getSystemService(Context.NOTIFICATION_SERVICE);
        if (nm == null) return;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            NotificationChannel channel = new NotificationChannel(
                    CHANNEL_ID, "App updates", NotificationManager.IMPORTANCE_LOW);
            channel.setDescription("Alerts when apps from your catalogs have updates.");
            nm.createNotificationChannel(channel);
        }

        // Tapping the notification opens App Fair's launcher activity.
        PendingIntent pending = null;
        Intent open = ctx.getPackageManager().getLaunchIntentForPackage(ctx.getPackageName());
        if (open != null) {
            open.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            int flags = PendingIntent.FLAG_UPDATE_CURRENT
                    | (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ? PendingIntent.FLAG_IMMUTABLE : 0);
            pending = PendingIntent.getActivity(ctx, 0, open, flags);
        }

        String title = (count == 1) ? "1 update available" : count + " updates available";
        Notification.Builder b = (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O)
                ? new Notification.Builder(ctx, CHANNEL_ID)
                : new Notification.Builder(ctx);
        b.setContentTitle(title)
                .setContentText("Open App Fair to review and update.")
                .setSmallIcon(ctx.getApplicationInfo().icon)
                .setAutoCancel(true);
        if (pending != null) b.setContentIntent(pending);
        nm.notify(NOTIFICATION_ID, b.build());
    }
}
