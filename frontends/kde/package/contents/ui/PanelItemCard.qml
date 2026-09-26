pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components as PlasmaComponents
import "../code/plasmoid-logic.mjs" as Logic

// One panel item as a card: a header, then each provider with a bar per
// quota window. Shared by the click popup (one card per account in use) and
// the tooltip (blocks: every item, pager: one), so both look the same.
Rectangle {
    id: card

    required property var applet
    // What the header prints: an account label or a provider name.
    property string title: ""
    property bool active: false
    // Provider objects in the report's accounts[].providers shape.
    property var providers: []

    Layout.fillWidth: true
    implicitHeight: cardCol.implicitHeight + Kirigami.Units.smallSpacing * 2
    radius: Kirigami.Units.cornerRadius
    color: card.active
        ? Qt.alpha(Kirigami.Theme.highlightColor, 0.12)
        : Qt.alpha(Kirigami.Theme.textColor, 0.05)
    border.width: 1
    border.color: card.active
        ? Kirigami.Theme.highlightColor
        : Qt.alpha(Kirigami.Theme.textColor, 0.2)

    ColumnLayout {
        id: cardCol
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Kirigami.Units.smallSpacing
        spacing: Kirigami.Units.smallSpacing / 2

        // Header
        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Kirigami.Icon {
                source: "user-identity"
                implicitWidth: Kirigami.Units.iconSizes.small
                implicitHeight: Kirigami.Units.iconSizes.small
                color: card.active
                    ? Kirigami.Theme.highlightColor
                    : Kirigami.Theme.textColor
            }

            PlasmaComponents.Label {
                Layout.fillWidth: true
                font.bold: true
                text: card.title
                elide: Text.ElideRight
                textFormat: Text.PlainText
            }

            Rectangle {
                visible: card.active
                implicitWidth: activeLbl.implicitWidth + Kirigami.Units.smallSpacing * 2
                implicitHeight: activeLbl.implicitHeight + Kirigami.Units.smallSpacing / 2
                radius: height / 2
                color: Kirigami.Theme.highlightColor

                PlasmaComponents.Label {
                    id: activeLbl
                    anchors.centerIn: parent
                    text: i18n("Ativa")
                    font.bold: true
                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                    color: Kirigami.Theme.highlightedTextColor
                }
            }
        }

        // Providers & Metrics
        Repeater {
            model: (card.providers && card.providers.length > 0)
                ? card.providers : []

            delegate: ColumnLayout {
                id: provCol
                required property var modelData
                Layout.fillWidth: true
                spacing: 2

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing

                    PlasmaComponents.Label {
                        text: provCol.modelData.name || provCol.modelData.id
                        font.bold: true
                        font.pointSize: Kirigami.Theme.smallFont.pointSize
                        color: Kirigami.Theme.highlightColor
                        textFormat: Text.PlainText
                    }
                }

                Repeater {
                    model: provCol.modelData.metrics || []

                    delegate: RowLayout {
                        id: metricRow
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing

                        PlasmaComponents.Label {
                            text: "↳ " + metricRow.modelData.label
                            font: Kirigami.Theme.smallFont
                            opacity: 0.85
                            Layout.minimumWidth: Kirigami.Units.gridUnit * 6
                            textFormat: Text.PlainText
                        }

                        UsageBar {
                            visible: metricRow.modelData.percent !== null
                            pct: metricRow.modelData.percent ?? 0
                            severity: metricRow.modelData.severity || "low"
                            colors: card.applet.colors
                            implicitWidth: Kirigami.Units.gridUnit * 5
                            Layout.alignment: Qt.AlignVCenter
                        }

                        PlasmaComponents.Label {
                            text: metricRow.modelData.value || (metricRow.modelData.percent + "%")
                            font.pointSize: Kirigami.Theme.smallFont.pointSize
                            font.bold: (metricRow.modelData.percent ?? 0) >= 90
                            color: Logic.severityColor(metricRow.modelData.severity, card.applet.colors) ?? Kirigami.Theme.textColor
                            textFormat: Text.PlainText
                        }
                    }
                }
            }
        }

        PlasmaComponents.Label {
            visible: !card.providers || card.providers.length === 0
            text: i18n("Nenhum provedor consultado ainda nesta conta.")
            font: Kirigami.Theme.smallFont
            opacity: 0.6
            textFormat: Text.PlainText
        }
    }
}
