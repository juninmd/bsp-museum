// Guarda de regressão: um `}` perdido num CSS faz todas as regras seguintes ficarem aninhadas dentro
// de um seletor qualquer (o merge da camada "Sons 3D" fez isso: `.gallery[hidden]` virou
// `.uv-list .gallery[hidden]` e a galeria passou a aparecer por baixo das outras seções).
import { describe, expect, test } from "bun:test";
import { Glob } from "bun";

/** Chaves fora de comentários e de strings (`content: "{"` não conta). */
function depthAfter(text: string): { depth: number; min: number } {
  const src = text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/g, '""');
  let depth = 0;
  let min = 0;
  for (const ch of src) {
    if (ch === "{") depth++;
    else if (ch === "}") {
      depth--;
      min = Math.min(min, depth);
    }
  }
  return { depth, min };
}

describe("CSS", () => {
  test("todo arquivo tem chaves balanceadas (nada fica aninhado por engano)", async () => {
    const files = [...new Glob("src/**/*.css").scanSync(".")];
    expect(files.length).toBeGreaterThan(0);
    for (const f of files) {
      const text = await Bun.file(f).text();
      const { depth, min } = depthAfter(text);
      expect({ file: f, depth }).toEqual({ file: f, depth: 0 });
      expect({ file: f, min }).toEqual({ file: f, min: 0 });
    }
  });
});
