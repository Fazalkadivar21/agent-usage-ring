import QtQuick
import QtQuick.Shapes

// One ring: a dim full-circle track plus an arc whose length is the REMAINING
// fraction. The arc starts at 12 o'clock, runs clockwise and shrinks back
// toward 12 o'clock as quota drains. Colour steps like a battery indicator.
Item {
    id: root

    property real radius: 10
    property real lineWidth: 3
    property real remaining: 1          // 0..1
    property bool hasData: true

    // Thresholds are percent remaining.
    property color colFull: "#5fbf6a"   // > 50
    property color colMid: "#e8c547"    // 25..50
    property color colLow: "#f0913a"    // 10..25
    property color colCrit: "#e5534b"   // < 10
    property color colNoData: "#888888"
    property real trackOpacity: 0.22

    readonly property color ringColor: {
        if (!hasData) return colNoData;
        const p = remaining * 100;
        if (p > 50) return colFull;
        if (p > 25) return colMid;
        if (p > 10) return colLow;
        return colCrit;
    }

    property color animatedColor: ringColor
    Behavior on animatedColor { ColorAnimation { duration: 600 } }

    property real sweep: hasData ? Math.max(0, Math.min(1, remaining)) * 360 : 0
    Behavior on sweep {
        NumberAnimation { duration: 900; easing.type: Easing.OutCubic }
    }

    implicitWidth: radius * 2 + lineWidth
    implicitHeight: implicitWidth

    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer

        ShapePath { // track
            strokeColor: Qt.rgba(root.animatedColor.r, root.animatedColor.g, root.animatedColor.b, root.trackOpacity)
            strokeWidth: root.lineWidth
            fillColor: "transparent"
            PathAngleArc {
                centerX: root.width / 2
                centerY: root.height / 2
                radiusX: root.radius
                radiusY: root.radius
                startAngle: 0
                sweepAngle: 360
            }
        }

        ShapePath { // remaining quota
            strokeColor: root.sweep > 0.5 ? root.animatedColor : "transparent"
            strokeWidth: root.lineWidth
            capStyle: ShapePath.RoundCap
            fillColor: "transparent"
            PathAngleArc {
                centerX: root.width / 2
                centerY: root.height / 2
                radiusX: root.radius
                radiusY: root.radius
                startAngle: -90
                sweepAngle: root.sweep
            }
        }
    }
}
