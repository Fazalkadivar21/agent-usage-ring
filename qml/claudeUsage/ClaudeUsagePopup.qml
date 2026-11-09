import qs.modules.common
import qs.modules.common.widgets
import qs.modules.ii.bar
import QtQuick
import QtQuick.Layouts

// Hover popup with the details. Uses ii's StyledPopup so it looks and
// positions like the battery/resources popups.
StyledPopup {
    id: root

    required property var usage   // the ClaudeUsage widget (data + helpers)

    component LimitSection: ColumnLayout {
        id: section
        required property string title
        required property string ring
        required property var win
        required property color dotColor
        spacing: 4

        RowLayout {
            spacing: 6
            Rectangle {
                implicitWidth: 9
                implicitHeight: 9
                radius: 5
                color: section.dotColor
            }
            StyledText {
                text: section.title
                font.weight: Font.DemiBold
                color: Appearance.colors.colOnSurfaceVariant
            }
            StyledText {
                text: section.ring
                font.pixelSize: Appearance.font.pixelSize.smaller
                color: Appearance.colors.colSubtext
            }
        }
        StyledPopupValueRow {
            Layout.fillWidth: true
            icon: "battery_android_full"
            label: "Remaining:"
            value: section.win ? `${Math.round(root.usage.effectiveRemaining(section.win))}%` : "–"
        }
        StyledPopupValueRow {
            Layout.fillWidth: true
            icon: "timer"
            label: "Resets in:"
            value: section.win ? root.usage.formatCountdown(section.win.resets_at) : "–"
        }
        StyledPopupValueRow {
            Layout.fillWidth: true
            icon: "event"
            label: "Resets at:"
            value: section.win ? root.usage.formatAbsolute(section.win.resets_at) : "–"
        }
    }

    ColumnLayout {
        anchors.centerIn: parent
        spacing: 10

        StyledPopupHeaderRow {
            icon: "data_usage"
            label: "Claude usage"
        }

        LimitSection {
            Layout.minimumWidth: 220
            title: "5-hour limit"
            ring: "outer ring"
            win: root.usage.fiveHour
            dotColor: root.usage.outerColor
        }

        LimitSection {
            Layout.minimumWidth: 220
            title: "Weekly limit"
            ring: "inner ring"
            win: root.usage.sevenDay
            dotColor: root.usage.innerColor
        }

        StyledText {
            visible: text.length > 0
            Layout.fillWidth: true
            Layout.maximumWidth: 260
            wrapMode: Text.Wrap
            font.pixelSize: Appearance.font.pixelSize.smaller
            color: root.usage.isStale || !root.usage.hasData ? Appearance.m3colors.m3error : Appearance.colors.colSubtext
            text: root.usage.statusLine
        }
    }
}
