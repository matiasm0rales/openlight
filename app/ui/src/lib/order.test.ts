import { describe, expect, it } from "vitest";
import { dropAt, move, sortByIds } from "./order";

describe("orden de una lista", () => {
  it("mueve un elemento a su lugar final y corre a los demás", () => {
    expect(move(["a", "b", "c", "d"], 0, 2)).toEqual(["b", "c", "a", "d"]);
    expect(move(["a", "b", "c", "d"], 3, 0)).toEqual(["d", "a", "b", "c"]);
    expect(move(["a", "b"], 1, 1)).toEqual(["a", "b"]);
  });

  it("no se sale de la lista ni cambia la original", () => {
    const list = ["a", "b", "c"];
    expect(move(list, 0, -1)).toEqual(list);
    expect(move(list, 2, 3)).toEqual(list);
    expect(move(list, 5, 0)).toEqual(list);
    move(list, 0, 2);
    expect(list).toEqual(["a", "b", "c"]);
  });

  it("ordena por una lista de ids; lo que no figura queda al final, como estaba", () => {
    const items = ["a", "b", "x", "c", "y"].map((id) => ({ id }));
    expect(sortByIds(items, ["c", "a", "b", "ya-no-está"]).map((i) => i.id)).toEqual(["c", "a", "b", "x", "y"]);
    expect(items.map((i) => i.id)).toEqual(["a", "b", "x", "c", "y"]);
  });
});

describe("dónde cae un foco arrastrado", () => {
  // Dos columnas de 400×300 con 20 de separación; el quinto queda solo en la tercera fila.
  const boxes = [0, 1, 2, 3, 4].map((i) => ({ left: (i % 2) * 420, top: Math.floor(i / 2) * 320, width: 400, height: 300 }));

  it("toma el lugar de la tarjeta que tiene debajo", () => {
    expect(dropAt(boxes, 0, 600, 100)).toBe(1);
    expect(dropAt(boxes, 4, 10, 10)).toBe(0);
    expect(dropAt(boxes, 1, 200, 700)).toBe(4);
  });

  it("en el espacio entre tarjetas cuenta la más cercana", () => {
    expect(dropAt(boxes, 0, 415, 100)).toBe(1);
    expect(dropAt(boxes, 4, 200, 305)).toBe(0);
    expect(dropAt(boxes, 4, 200, 318)).toBe(2);
  });

  it("sobre su propio lugar o lejos de la lista no cambia nada", () => {
    expect(dropAt(boxes, 2, 100, 400)).toBeNull();
    expect(dropAt(boxes, 0, -200, 100)).toBeNull();
    expect(dropAt(boxes, 0, 200, 1200)).toBeNull();
    expect(dropAt([], 0, 0, 0)).toBeNull();
  });
});
