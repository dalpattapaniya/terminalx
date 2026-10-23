import XCTest

final class ScreenCapturePermissionPreflightSafetyTests: XCTestCase {
    func testPermissionPreflightCannotRequestOrCaptureTheScreen() throws {
        let source = try computerUseSource()
        let start = try XCTUnwrap(source.range(of: "private func screenCaptureTrusted()"))
        let end = try XCTUnwrap(
            source.range(of: "private func openAccessibilitySettings()", range: start.upperBound..<source.endIndex)
        )
        let preflightSource = source[start.lowerBound..<end.lowerBound]

        XCTAssertTrue(preflightSource.contains("CGPreflightScreenCaptureAccess()"))
        XCTAssertTrue(preflightSource.contains("timeoutMs: 2_000"))
        XCTAssertFalse(preflightSource.contains("CGRequestScreenCaptureAccess()"))
        XCTAssertFalse(preflightSource.contains("CGWindowListCreateImage"))
        XCTAssertFalse(preflightSource.contains("SCScreenshotManager"))
    }

    func testGrantOpensSettingsAndDragAssistantWithoutRequestingCaptureAccess() throws {
        let source = try computerUseSource()
        // Source checks deliberately avoid calling TCC or opening real Settings.
        XCTAssertFalse(source.contains("CGRequestScreenCaptureAccess"))
        let start = try XCTUnwrap(source.range(of: "func openPermission(_ permission: PermissionKind)"))
        let end = try XCTUnwrap(source.range(of: "func refreshPermissions()", range: start.upperBound..<source.endIndex))
        let grantSource = source[start.lowerBound..<end.lowerBound]
        XCTAssertTrue(grantSource.contains("permission.openSettings()"))
        XCTAssertTrue(grantSource.contains("showDragAssistant(for: permission)"))

        let settingsStart = try XCTUnwrap(source.range(of: "func openSettings()"))
        let settingsEnd = try XCTUnwrap(source.range(of: "private final class PermissionView", range: settingsStart.upperBound..<source.endIndex))
        let settingsSource = source[settingsStart.lowerBound..<settingsEnd.lowerBound]
        XCTAssertTrue(settingsSource.contains("case .accessibility:\n            openAccessibilitySettings()"))
        XCTAssertTrue(settingsSource.contains("case .screenshots:"))
        XCTAssertTrue(settingsSource.contains("openScreenRecordingSettings()"))
        XCTAssertTrue(source.contains("Drag TerminalX Computer Use Helper into the list above to allow Screenshots."))
    }

    private func computerUseSource() throws -> String {
        let testFile = URL(fileURLWithPath: #filePath)
        let packageRoot = testFile
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let mainPath = packageRoot
            .appendingPathComponent("Sources")
            .appendingPathComponent("TerminalXComputerUseMacOS")
            .appendingPathComponent("main.swift")
        return try String(contentsOf: mainPath, encoding: .utf8)
    }
}
