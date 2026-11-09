import qs.modules.common
import qs.modules.common.functions
import qs.modules.ii.bar
import Quickshell
import Quickshell.Io
import QtQuick

// Claude usage rings for the ii bar.
//   outer ring = 5-hour limit, inner ring = weekly limit
//   rings show REMAINING quota and shrink as it drains.
// Data comes from the claude-usage-ring daemon, which writes
// $XDG_CACHE_HOME/claude-usage/usage.json (override with $CLAUDE_USAGE_FILE).
MouseArea {
    id: root

    property string dataPath: Quickshell.env("CLAUDE_USAGE_FILE")
        || FileUtils.trimFileProtocol(`${Directories.genericCache}/claude-usage/usage.json`)
    property real ringSize: 20
    property real lineWidth: 2.5

    // ---- data
    property var snapshot: null
    property bool fileMissing: false
    property date now: new Date()

    readonly property var fiveHour: snapshot?.five_hour ?? null
    readonly property var sevenDay: snapshot?.seven_day ?? null
    readonly property bool hasData: fiveHour !== null && sevenDay !== null
    // Daemon reports stale, or it has missed its own next poll by > 2 min (dead daemon).
    readonly property bool daemonLate: snapshot !== null && !!snapshot.next_poll_at
        && (now.getTime() - Date.parse(snapshot.next_poll_at)) > 120000
    readonly property bool isStale: hasData && (snapshot.status !== "ok" || daemonLate)

    readonly property color outerColor: outerRing.ringColor
    readonly property color innerColor: innerRing.ringColor

    // Once a window's reset time has passed the quota is full again, even if
    // the daemon has not polled yet.
    function effectiveRemaining(win) {
        if (!win) return 0;
        if (win.resets_at && Date.parse(win.resets_at) <= now.getTime()) return 100;
        return win.remaining_pct;
    }

    function formatDuration(ms) {
        const totalMin = Math.max(0, Math.round(ms / 60000));
        const d = Math.floor(totalMin / 1440);
        const h = Math.floor((totalMin % 1440) / 60);
        const m = totalMin % 60;
        if (d > 0) return `${d}d ${h}h`;
        if (h > 0) return `${h}h ${m}m`;
        return `${m}m`;
    }

    function formatCountdown(iso) {
        if (!iso) return "not started";
        const ms = Date.parse(iso) - now.getTime();
        return ms <= 0 ? "now" : formatDuration(ms);
    }

    function formatAbsolute(iso) {
        if (!iso) return "–";
        // API reports e.g. 09:59:59.87; round to the nearest minute for display.
        const t = new Date(Math.round(Date.parse(iso) / 60000) * 60000);
        const time = Qt.formatTime(t, "HH:mm");
        const today = new Date(now.getTime());
        if (t.toDateString() === today.toDateString()) return `today ${time}`;
        const tomorrow = new Date(today.getTime() + 86400000);
        if (t.toDateString() === tomorrow.toDateString()) return `tomorrow ${time}`;
        return Qt.formatDateTime(t, "ddd d MMM, HH:mm");
    }

    readonly property string statusLine: {
        if (fileMissing) return "No data: is claude-usage-ring running?";
        if (snapshot === null) return "Loading…";
        if (!hasData) return `No data: ${snapshot.error ?? "unknown error"}`;
        const age = snapshot.fetched_at ? formatDuration(now.getTime() - Date.parse(snapshot.fetched_at)) : "?";
        const src = snapshot.source === "mock" ? " (mock data)" : "";
        if (daemonLate) return `Stale: daemon not updating (data ${age} old)${src}`;
        if (snapshot.status !== "ok") return `Stale (${age} old): ${snapshot.error ?? "fetch failed"}${src}`;
        return `Updated ${age} ago${src}`;
    }

    // ---- layout / interaction
    // Sits inside an existing BarGroup (next to the battery), so no own background.
    implicitWidth: ringSize + horizontalPadding * 2
    implicitHeight: ringSize
    property real horizontalPadding: 2
    hoverEnabled: !Config.options.bar.tooltips.clickToShow
    // Let clicks fall through to the bar (e.g. right-sidebar toggle) unless
    // the user configured click-to-show tooltips.
    acceptedButtons: Config.options.bar.tooltips.clickToShow ? Qt.LeftButton : Qt.NoButton

    Item {
        anchors.centerIn: parent
        implicitWidth: root.ringSize
        implicitHeight: root.ringSize
        opacity: root.isStale ? 0.45 : 1
        Behavior on opacity { NumberAnimation { duration: 300 } }

        ClaudeUsageRing {
            id: outerRing
            anchors.centerIn: parent
            lineWidth: root.lineWidth
            radius: root.ringSize / 2 - lineWidth / 2
            hasData: root.hasData
            remaining: root.effectiveRemaining(root.fiveHour) / 100
        }
        ClaudeUsageRing {
            id: innerRing
            anchors.centerIn: parent
            lineWidth: root.lineWidth
            radius: outerRing.radius - lineWidth - 1
            hasData: root.hasData
            remaining: root.effectiveRemaining(root.sevenDay) / 100
        }
    }

    // ---- data source
    FileView {
        id: file
        path: root.dataPath
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            root.fileMissing = false;
            try {
                root.snapshot = JSON.parse(text());
            } catch (e) {
                // Partial/garbled read: keep the previous value.
            }
        }
        onLoadFailed: error => {
            if (error === FileViewError.FileNotFound) root.fileMissing = true;
        }
    }

    // Clock for countdowns/staleness, plus a periodic re-read in case an
    // inotify watch is lost (e.g. the cache dir was recreated).
    Timer {
        interval: 30000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: {
            root.now = new Date();
            file.reload();
        }
    }

    ClaudeUsagePopup {
        hoverTarget: root
        usage: root
        onActiveChanged: if (active) root.now = new Date()
    }
}
