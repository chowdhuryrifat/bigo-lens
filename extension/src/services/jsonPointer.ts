export interface TextPosition {
  line: number;
  character: number;
}

export interface TextRange {
  start: TextPosition;
  end: TextPosition;
}

export interface OffsetRange {
  startOffset: number;
  endOffset: number;
}

/**
 * Decodes a JSON Pointer token according to RFC 6901:
 * `~1` is decoded to `/` and `~0` is decoded to `~`.
 */
export function decodePointerToken(token: string): string {
  return token.replace(/~1/g, '/').replace(/~0/g, '~');
}

/**
 * Splits a JSON Pointer into decoded reference tokens.
 * Leading `/` is stripped.
 */
export function parsePointer(pointer: string): string[] {
  if (!pointer || pointer === '/') {
    return [];
  }
  const clean = pointer.startsWith('/') ? pointer.slice(1) : pointer;
  return clean.split('/').map(decodePointerToken);
}

interface JsonStringNode {
  kind: 'string';
  value: string;
  start: number;
  end: number;
}

interface JsonObjectNode {
  kind: 'object';
  start: number;
  end: number;
  properties: Array<{
    key: JsonStringNode;
    value: JsonNode;
    start: number;
    end: number;
  }>;
}

interface JsonArrayNode {
  kind: 'array';
  start: number;
  end: number;
  elements: JsonNode[];
}

interface JsonPrimitiveNode {
  kind: 'primitive';
  start: number;
  end: number;
}

type JsonNode = JsonStringNode | JsonObjectNode | JsonArrayNode | JsonPrimitiveNode;

class JsonParser {
  private pos = 0;

  constructor(private readonly text: string) {}

  parse(): JsonNode | undefined {
    this.skipWhitespace();
    if (this.pos >= this.text.length) {
      return undefined;
    }
    return this.parseValue();
  }

  private skipWhitespace(): void {
    while (this.pos < this.text.length) {
      const ch = this.text.charCodeAt(this.pos);
      if (ch === 0x20 || ch === 0x09 || ch === 0x0a || ch === 0x0d) {
        this.pos++;
      } else {
        break;
      }
    }
  }

  private parseValue(): JsonNode | undefined {
    this.skipWhitespace();
    if (this.pos >= this.text.length) {
      return undefined;
    }
    const ch = this.text[this.pos];
    if (ch === '{') {
      return this.parseObject();
    }
    if (ch === '[') {
      return this.parseArray();
    }
    if (ch === '"') {
      return this.parseString();
    }
    return this.parsePrimitive();
  }

  private parseObject(): JsonObjectNode | undefined {
    const start = this.pos;
    this.pos++; // skip '{'
    const properties: JsonObjectNode['properties'] = [];

    this.skipWhitespace();
    while (this.pos < this.text.length && this.text[this.pos] !== '}') {
      this.skipWhitespace();
      if (this.text[this.pos] !== '"') {
        break;
      }
      const propStart = this.pos;
      const key = this.parseString();
      if (!key) {
        break;
      }
      this.skipWhitespace();
      if (this.text[this.pos] === ':') {
        this.pos++; // skip ':'
      }
      this.skipWhitespace();
      const val = this.parseValue();
      if (!val) {
        break;
      }
      properties.push({
        key,
        value: val,
        start: propStart,
        end: val.end,
      });

      this.skipWhitespace();
      if (this.text[this.pos] === ',') {
        this.pos++;
        this.skipWhitespace();
      } else {
        break;
      }
    }

    if (this.pos < this.text.length && this.text[this.pos] === '}') {
      this.pos++;
    }
    return { kind: 'object', start, end: this.pos, properties };
  }

  private parseArray(): JsonArrayNode | undefined {
    const start = this.pos;
    this.pos++; // skip '['
    const elements: JsonNode[] = [];

    this.skipWhitespace();
    while (this.pos < this.text.length && this.text[this.pos] !== ']') {
      const val = this.parseValue();
      if (!val) {
        break;
      }
      elements.push(val);
      this.skipWhitespace();
      if (this.text[this.pos] === ',') {
        this.pos++;
        this.skipWhitespace();
      } else {
        break;
      }
    }

    if (this.pos < this.text.length && this.text[this.pos] === ']') {
      this.pos++;
    }
    return { kind: 'array', start, end: this.pos, elements };
  }

  private parseString(): JsonStringNode | undefined {
    const start = this.pos;
    this.pos++; // skip opening '"'
    let value = '';
    while (this.pos < this.text.length) {
      const ch = this.text[this.pos];
      if (ch === '"') {
        this.pos++; // skip closing '"'
        return { kind: 'string', value, start, end: this.pos };
      }
      if (ch === '\\' && this.pos + 1 < this.text.length) {
        this.pos++;
        const esc = this.text[this.pos] ?? '';
        switch (esc) {
          case '"':
          case '\\':
          case '/':
            value += esc;
            break;
          case 'b':
            value += '\b';
            break;
          case 'f':
            value += '\f';
            break;
          case 'n':
            value += '\n';
            break;
          case 'r':
            value += '\r';
            break;
          case 't':
            value += '\t';
            break;
          case 'u':
            value += String.fromCharCode(parseInt(this.text.substring(this.pos + 1, this.pos + 5), 16) || 0);
            this.pos += 4;
            break;
          default:
            value += esc;
            break;
        }
      } else if (typeof ch === 'string') {
        value += ch;
      }
      this.pos++;
    }
    return { kind: 'string', value, start, end: this.pos };
  }

  private parsePrimitive(): JsonPrimitiveNode {
    const start = this.pos;
    while (this.pos < this.text.length) {
      const ch = this.text[this.pos];
      if (ch === ',' || ch === '}' || ch === ']' || ch === ' ' || ch === '\t' || ch === '\n' || ch === '\r') {
        break;
      }
      this.pos++;
    }
    return { kind: 'primitive', start, end: this.pos };
  }
}

/**
 * Converts a string character offset to a 0-based { line, character } position.
 */
export function offsetToPosition(text: string, offset: number): TextPosition {
  const clamped = Math.max(0, Math.min(offset, text.length));
  let line = 0;
  let character = 0;
  for (let i = 0; i < clamped; i++) {
    if (text[i] === '\n') {
      line++;
      character = 0;
    } else {
      character++;
    }
  }
  return { line, character };
}

/**
 * Finds the character offset range corresponding to a JSON pointer in JSON source text.
 * When pointing to an object property, returns the key's token range so that the property
 * name is underlined.
 */
export function findJsonPointerOffset(text: string, pointer: string): OffsetRange | undefined {
  const root = new JsonParser(text).parse();
  if (!root) {
    return undefined;
  }
  const tokens = parsePointer(pointer);
  if (tokens.length === 0) {
    return { startOffset: root.start, endOffset: root.end };
  }

  let current: JsonNode = root;
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    if (token === undefined) {
      continue;
    }
    const isLast = i === tokens.length - 1;

    if (current.kind === 'object') {
      const prop = current.properties.find((p) => p.key.value === token);
      if (!prop) {
        return undefined;
      }
      if (isLast) {
        // Return key offset range so the entry property is underlined
        return { startOffset: prop.key.start, endOffset: prop.key.end };
      }
      current = prop.value;
    } else if (current.kind === 'array') {
      const index = parseInt(token, 10);
      if (isNaN(index) || index < 0 || index >= current.elements.length) {
        return undefined;
      }
      const elem = current.elements[index];
      if (!elem) {
        return undefined;
      }
      if (isLast) {
        return { startOffset: elem.start, endOffset: elem.end };
      }
      current = elem;
    } else {
      return undefined;
    }
  }

  return { startOffset: current.start, endOffset: current.end };
}

/**
 * Finds the 0-based { start: { line, character }, end: { line, character } } range
 * corresponding to a JSON pointer in JSON source text.
 */
export function findJsonPointerRange(text: string, pointer: string): TextRange | undefined {
  const offset = findJsonPointerOffset(text, pointer);
  if (!offset) {
    return undefined;
  }
  return {
    start: offsetToPosition(text, offset.startOffset),
    end: offsetToPosition(text, offset.endOffset),
  };
}
