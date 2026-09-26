pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components as PlasmaComponents
import "../code/plasmoid-logic.mjs" as Logic

ColumnLayout {
    id: root

    required property var applet

    // Every account in use right now, each narrowed to the providers it is
    // the current session for. The click popup always shows all of them.
    readonly property var allAccounts: Logic.inUseAccounts(root.applet.accounts)

    spacing: Kirigami.Units.smallSpacing

    Repeater {
        model: root.allAccounts

        delegate: PanelItemCard {
            required property var modelData
            Layout.fillWidth: true
            applet: root.applet
            title: Logic.formatAccount(modelData.label, root.applet.showFullEmail)
            active: modelData.active
            providers: modelData.providers || []
        }
    }
}
