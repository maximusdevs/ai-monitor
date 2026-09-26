pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components as PlasmaComponents
import "../code/plasmoid-logic.mjs" as Logic

// A custom toolTipItem rather than toolTipMainText/toolTipSubText: the default
// tooltip hardcodes textFormat: Text.PlainText on the main text and caps the
// sub text at 8 lines in a single wrapped Label, which cannot express one row
// per quota window with a bar.
//
// [display] hover_mode picks the layout: `blocks` shows every panel item as a
// card, `pager` one item with its position. A Plasma tooltip takes no clicks,
// so the pager's ◀ ▶ are driven by the mouse wheel over the panel instead.
Item {
    id: tip

    required property var applet

    readonly property var panel: tip.applet.panel
    readonly property bool hasItems: tip.panel.items.length > 0
    readonly property bool pager: tip.panel.hover === "pager"

    // Tooltips use the Window colour set. Copying what DefaultToolTip.qml does
    // matters: without `inherit: false` the set is overwritten by the parent and
    // the text picks up panel colours, which on some themes is invisible.
    Kirigami.Theme.colorSet: Kirigami.Theme.Window
    Kirigami.Theme.inherit: false

    // The tooltip does not size a custom item for us, so the floor has to be
    // applied HERE. Layout attached properties only take effect on the direct
    // child of a layout, and this parent is a plain Item.
    readonly property Item content: tip.hasItems ? cards : rows
    implicitWidth: Math.max(tip.content.implicitWidth, Kirigami.Units.gridUnit * 20)
    implicitHeight: tip.content.implicitHeight

    // An older binary without `panel`: the report's rows, as before.
    UsageRows {
        id: rows
        visible: !tip.hasItems
        applet: tip.applet
        anchors.fill: parent
    }

    ColumnLayout {
        id: cards
        visible: tip.hasItems
        anchors.fill: parent
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            visible: tip.pager && tip.panel.items.length > 1
            Layout.fillWidth: true

            PlasmaComponents.Label {
                text: "◀"
                opacity: 0.6
                textFormat: Text.PlainText
            }
            PlasmaComponents.Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                font: Kirigami.Theme.smallFont
                opacity: 0.8
                text: i18n("%1 / %2 · scroll to change",
                    Logic.wrapIndex(tip.applet.panelPosition, tip.panel.items.length) + 1,
                    tip.panel.items.length)
                textFormat: Text.PlainText
            }
            PlasmaComponents.Label {
                text: "▶"
                opacity: 0.6
                textFormat: Text.PlainText
            }
        }

        Repeater {
            model: tip.pager ? (tip.applet.pagerItem ? [tip.applet.pagerItem] : []) : tip.panel.items

            delegate: PanelItemCard {
                required property var modelData
                Layout.fillWidth: true
                applet: tip.applet
                title: modelData.title
                active: modelData.active && tip.panel.unit === "account"
                providers: modelData.providers
            }
        }
    }
}
