import { fireEvent, screen, waitFor, within } from "@testing-library/react";

/** Open the composer's options menu and return its Permissions entry (the mode's label and lock are on it). */
export async function permissionEntry(): Promise<HTMLElement> {
  if (!screen.queryByTestId("permission-mode")) fireEvent.keyDown(screen.getByRole("button", { name: "More options", hidden: true }), { key: "Enter" });
  return screen.findByTestId("permission-mode");
}

/** Open the Permissions submenu and return it. */
export async function openPermissionModes(): Promise<HTMLElement> {
  fireEvent.keyDown(await permissionEntry(), { key: "ArrowRight" });
  return waitFor(() => {
    const menu = screen.getAllByRole("menu").find((m) => within(m).queryAllByRole("menuitemradio").length > 0);
    if (!menu) throw new Error("the Permissions submenu is not open");
    return menu;
  });
}
