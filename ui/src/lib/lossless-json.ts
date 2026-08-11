export class ExactJsonNumber {
  readonly lexeme: string;

  constructor(lexeme: string) {
    this.lexeme = lexeme;
    Object.freeze(this);
  }
}

export function isExactJsonNumber(value: unknown): value is ExactJsonNumber {
  return value instanceof ExactJsonNumber;
}

export function strictJsonParse(content: string): unknown {
  class Parser {
    private index = 0;

    constructor(private readonly source: string) {}

    parse(): unknown {
      const value = this.value();
      this.space();
      if (this.index !== this.source.length) this.fail("Unexpected trailing content.");
      return value;
    }

    private value(): unknown {
      this.space();
      const char = this.source[this.index];
      if (char === "{") return this.object();
      if (char === "[") return this.array();
      if (char === '"') return this.string();
      if (char === "t") return this.literal("true", true);
      if (char === "f") return this.literal("false", false);
      if (char === "n") return this.literal("null", null);
      if (char === "-" || (char >= "0" && char <= "9")) return this.number();
      this.fail("Expected a JSON value.");
    }

    private object(): Record<string, unknown> {
      this.index += 1;
      const result = Object.create(null) as Record<string, unknown>;
      const keys = new Set<string>();
      this.space();
      if (this.take("}")) return result;
      while (true) {
        this.space();
        if (this.source[this.index] !== '"') this.fail("Expected an object key.");
        const key = this.string();
        if (keys.has(key)) this.fail(`Duplicate object key ${JSON.stringify(key)}.`);
        keys.add(key);
        this.space();
        if (!this.take(":")) this.fail("Expected ':' after an object key.");
        Object.defineProperty(result, key, {
          value: this.value(), enumerable: true, writable: true, configurable: true,
        });
        this.space();
        if (this.take("}")) return result;
        if (!this.take(",")) this.fail("Expected ',' or '}' in an object.");
      }
    }

    private array(): unknown[] {
      this.index += 1;
      const result: unknown[] = [];
      this.space();
      if (this.take("]")) return result;
      while (true) {
        result.push(this.value());
        this.space();
        if (this.take("]")) return result;
        if (!this.take(",")) this.fail("Expected ',' or ']' in an array.");
      }
    }

    private string(): string {
      const start = this.index;
      this.index += 1;
      while (this.index < this.source.length) {
        const char = this.source[this.index];
        if (char === '"') {
          this.index += 1;
          return JSON.parse(this.source.slice(start, this.index)) as string;
        }
        if (char === "\\") { this.index += 2; continue; }
        if (char.charCodeAt(0) < 0x20) this.fail("Unescaped control character in a string.");
        this.index += 1;
      }
      this.fail("Unterminated JSON string.");
    }

    private number(): ExactJsonNumber {
      const match = this.source.slice(this.index).match(/^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/);
      if (!match) this.fail("Invalid JSON number.");
      this.index += match[0].length;
      return new ExactJsonNumber(match[0]);
    }

    private literal<T>(text: string, value: T): T {
      if (!this.source.startsWith(text, this.index)) this.fail(`Expected '${text}'.`);
      this.index += text.length;
      return value;
    }

    private take(char: string): boolean {
      if (this.source[this.index] !== char) return false;
      this.index += 1;
      return true;
    }

    private space(): void {
      while (/\s/.test(this.source[this.index] ?? "")) this.index += 1;
    }

    private fail(message: string): never {
      throw new Error(`${message} (byte ${new TextEncoder().encode(this.source.slice(0, this.index)).length})`);
    }
  }

  return new Parser(content).parse();
}

export function exactJsonStringify(value: unknown, space = 0): string {
  const indent = typeof space === "number" ? " ".repeat(Math.min(10, Math.max(0, space))) : "";
  const render = (current: unknown, depth: number): string => {
    if (isExactJsonNumber(current)) return current.lexeme;
    if (current === null) return "null";
    if (typeof current === "string" || typeof current === "boolean" || typeof current === "number") return JSON.stringify(current);
    if (Array.isArray(current)) {
      const values = current.map((item) => render(item, depth + 1));
      if (!indent) return `[${values.join(",")}]`;
      if (!values.length) return "[]";
      const pad = indent.repeat(depth + 1);
      return `[\n${pad}${values.join(`,\n${pad}`)}\n${indent.repeat(depth)}]`;
    }
    if (current !== null && typeof current === "object") {
      const values = Object.keys(current).map((key) => `${JSON.stringify(key)}:${indent ? " " : ""}${render((current as Record<string, unknown>)[key], depth + 1)}`);
      if (!indent) return `{${values.join(",")}}`;
      if (!values.length) return "{}";
      const pad = indent.repeat(depth + 1);
      return `{\n${pad}${values.join(`,\n${pad}`)}\n${indent.repeat(depth)}}`;
    }
    return "null";
  };
  return render(value, 0);
}

export function ownValueAt(value: unknown, pointer: string): unknown {
  if (!pointer || pointer === "/") return value;
  return pointer.split("/").slice(1).reduce<unknown>((current, segment) => {
    if (current === null || typeof current !== "object") return undefined;
    const key = segment.replace(/~1/g, "/").replace(/~0/g, "~");
    return Object.hasOwn(current, key) ? (current as Record<string, unknown>)[key] : undefined;
  }, value);
}
