// The row list the tooltip shows. Same rows as the popup, without the popup's
// chrome: no tab strip and no action buttons, because a tooltip cannot be
// clicked.
import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components as PlasmaComponents
import "../code/plasmoid-logic.mjs" as Logic

ColumnLayout {
    id: rows

    required property var applet

    readonly property var entries: (rows.applet.report && rows.applet.report.entries) || []
    readonly property var entry: rows.applet.entry
    readonly property string status: rows.applet.statusMessage()

    spacing: Kirigami.Units.smallSpacing

    Kirigami.Heading {
        Layout.fillWidth: true
        level: 4
        elide: Text.ElideRight
        text: {
            if (rows.entries.length > 1) {
                return rows.entries.map(function(e) { return e.label; }).join(" + ");
            }
            return rows.entry ? rows.entry.label : i18n("AI Monitor");
        }
        textFormat: Text.PlainText
    }

    PlasmaComponents.Label {
        Layout.fillWidth: true
        visible: text !== ""
        elide: Text.ElideRight
        font: Kirigami.Theme.smallFont
        opacity: 0.7
        text: {
            if (rows.entries.length > 1) {
                return rows.entries.map(function(e) {
                    return e.plan ? (e.label + ": " + e.plan) : e.label;
                }).join(" · ");
            }
            return rows.entry ? (rows.entry.plan || "") : "";
        }
        textFormat: Text.PlainText
    }

    PlasmaComponents.Label {
        Layout.fillWidth: true
        visible: text !== ""
        wrapMode: Text.WordWrap
        font: Kirigami.Theme.smallFont
        color: rows.applet.statusIsUrgent()
            ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
        text: rows.status
        textFormat: Text.PlainText
    }

    Repeater {
        model: rows.entries.length > 0 ? rows.entries : (rows.entry ? [rows.entry] : [])

        delegate: ColumnLayout {
            id: entrySection
            required property var modelData
            required property int index
            readonly property var list: Logic.detailRows(modelData, rows.applet.showExtraModels)

            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            visible: list.length > 0

            PlasmaComponents.Label {
                Layout.fillWidth: true
                visible: rows.entries.length > 1
                font.pointSize: Kirigami.Theme.smallFont.pointSize
                font.bold: true
                opacity: 0.8
                text: entrySection.modelData.label.toUpperCase()
                textFormat: Text.PlainText
            }

            Repeater {
                model: entrySection.list

                UsageRow {
                    required property var modelData

                    Layout.fillWidth: true
                    row: modelData
                    colors: rows.applet.colors
                    resetText: rows.applet.resetText(modelData.resetAt)
                    showBar: true
                }
            }
        }
    }

    PlasmaComponents.Label {
        Layout.fillWidth: true
        visible: text !== ""
        horizontalAlignment: Text.AlignHCenter
        font: Kirigami.Theme.smallFont
        opacity: 0.6
        text: rows.applet.updatedText()
        textFormat: Text.PlainText
    }
}
