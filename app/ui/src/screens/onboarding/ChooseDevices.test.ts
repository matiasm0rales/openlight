// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { setLocale } from "../../lib/i18n/index.svelte";

const failure = {
  code: "device_not_found_on_lan",
  title: "error.device_not_found_on_lan.title",
  detail: "error.device_not_found_on_lan.detail",
  params: {},
  actions: [{ kind: "retry" }, { kind: "show_help", value: "device_troubleshooting" }],
};

const light = {
  id: "bulb-1",
  name: "Foco de prueba",
  category: "dj",
  product_id: "p",
  is_light: true,
  online: true,
  capabilities: { dp_switch: "20", dp_mode: "21", dp_bright: null, dp_temp: null, dp_colour: ["24", "v2_hsv12"], dp_control: null, modes: ["colour"] },
  lan: { id: "bulb-1", ip: "192.0.2.5", version: "3.5", product_key: null },
};

const commands = vi.hoisted(() => ({
  cloudListDevices: vi.fn(),
  discoverLan: vi.fn(),
  deviceAdd: vi.fn(),
  deviceRemove: vi.fn(),
  devicesList: vi.fn(),
}));

vi.mock("../../lib/ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/ipc")>()),
  commands,
  on: vi.fn(async () => () => {}),
}));

import ChooseDevices from "./ChooseDevices.svelte";

setLocale("es");

beforeEach(() => {
  vi.clearAllMocks();
  commands.cloudListDevices.mockResolvedValue([light]);
  commands.discoverLan.mockResolvedValue([light.lan]);
  commands.deviceRemove.mockResolvedValue(null);
  commands.devicesList.mockResolvedValue([]);
});

describe("ChooseDevices: errores al agregar un foco", () => {
  it("'Buscar de nuevo' vuelve a buscar en la red y repite el agregado en vez de solo cerrar el error", async () => {
    // La primera vez el foco no aparece; la segunda sí
    commands.deviceAdd.mockRejectedValueOnce(failure).mockResolvedValueOnce({ id: "bulb-1" });
    const { component } = render(ChooseDevices, { valid: false });
    await screen.findByText("Foco de prueba");
    await waitFor(() => expect(commands.discoverLan).toHaveBeenCalledTimes(1));

    expect(await component.next()).toBe(false);
    const alert = await screen.findByRole("alert");
    expect(alert.getAttribute("data-code")).toBe("device_not_found_on_lan");
    expect(commands.deviceAdd).toHaveBeenCalledTimes(1);

    await fireEvent.click(within(alert).getByRole("button", { name: "Buscar de nuevo" }));

    await waitFor(() => expect(commands.deviceAdd).toHaveBeenCalledTimes(2));
    expect(commands.discoverLan).toHaveBeenCalledTimes(2);
    // El mismo foco, no otro, y el error desaparece porque ya se agregó
    expect(commands.deviceAdd.mock.calls[1][0]).toMatchObject({ id: "bulb-1" });
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });
});
