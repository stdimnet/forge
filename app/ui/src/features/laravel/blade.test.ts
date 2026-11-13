import { describe, expect, it } from "vitest";
import { StringStream, type StreamParser } from "@codemirror/language";
import { bladeParser } from "./blade";

/** Tokenize a document and return `[text, style]` pairs, skipping whitespace-only tokens. */
function tokens<S>(parser: StreamParser<S>, doc: string): [string, string | null][] {
  const state = parser.startState!(2);
  const out: [string, string | null][] = [];
  for (const line of doc.split("\n")) {
    if (!line) parser.blankLine?.(state, 2);
    const stream = new StringStream(line, 4, 2);
    while (!stream.eol()) {
      const style = parser.token(stream, state);
      const text = stream.current();
      if (stream.pos === stream.start) throw new Error(`no progress at "${line.slice(stream.pos)}"`);
      if (text.trim()) out.push([text.trim(), style ?? null]);
      stream.start = stream.pos;
    }
  }
  return out;
}

const blade = (doc: string, custom: string[] = []) => tokens(bladeParser({ customDirectives: () => custom }), doc);

describe("Blade highlighting", () => {
  it("highlights echoes as PHP", () => {
    const t = blade("<h1>{{ $order->total_price }}</h1>");
    expect(t).toEqual([
      ["<", "angleBracket"],
      ["h1", "tagName"],
      [">", "angleBracket"],
      ["{{", "processingInstruction"],
      ["$order", "variableName"],
      ["->", "operator"],
      ["total_price", "propertyName"],
      ["}}", "processingInstruction"],
      ["</", "angleBracket"],
      ["h1", "tagName"],
      [">", "angleBracket"],
    ]);
  });

  it("highlights raw echoes, function calls and strings", () => {
    const t = blade("{!! route('orders.show', ['order' => $order]) !!}");
    expect(t).toContainEqual(["{!!", "processingInstruction"]);
    expect(t).toContainEqual(["route", "variableName.function"]);
    expect(t).toContainEqual(["'orders.show'", "string"]);
    expect(t).toContainEqual(["=>", "operator"]);
    expect(t.at(-1)).toEqual(["!!}", "processingInstruction"]);
  });

  it("highlights directives with PHP arguments", () => {
    const t = blade("@foreach ($orders as $order)\n  <li>{{ $loop->index }}</li>\n@endforeach");
    expect(t[0]).toEqual(["@foreach", "keyword"]);
    expect(t).toContainEqual(["as", "keyword"]);
    expect(t).toContainEqual(["$orders", "variableName"]);
    expect(t).toContainEqual(["@endforeach", "keyword"]);
    expect(t).toContainEqual(["li", "tagName"]);
  });

  it("keeps nested parentheses inside directive arguments", () => {
    const t = blade("@if (count($items) > 0) yes @endif");
    expect(t).toContainEqual(["count", "variableName.function"]);
    expect(t).toContainEqual(["yes", null]);
    expect(t.at(-1)).toEqual(["@endif", "keyword"]);
  });

  it("highlights comments, @php blocks and escapes", () => {
    const t = blade("{{-- note {{ $x }} --}}\n@php\n  $total = Str::upper('a');\n@endphp\n@{{ raw }} mail@example.com");
    expect(t[0]).toEqual(["{{-- note {{ $x }} --}}", "comment"]);
    expect(t).toContainEqual(["@php", "keyword"]);
    expect(t).toContainEqual(["Str", "className"]);
    expect(t).toContainEqual(["upper", "propertyName.function"]);
    expect(t).toContainEqual(["@endphp", "keyword"]);
    expect(t.filter(([, s]) => s === "keyword").map(([x]) => x)).toEqual(["@php", "@endphp"]);
  });

  it("highlights components and bound attributes", () => {
    const t = blade('<x-alert type="error" :message="$errors->first(\'email\')" class="p-{{ $size }}" />');
    expect(t).toContainEqual(["x-alert", "className"]);
    expect(t).toContainEqual([":message", "attributeName"]);
    expect(t).toContainEqual(["$errors", "variableName"]);
    expect(t).toContainEqual(["first", "propertyName.function"]);
    expect(t).toContainEqual(["$size", "variableName"]);
    expect(t).toContainEqual(["{{", "processingInstruction"]);
    expect(t.at(-1)).toEqual(["/>", "angleBracket"]);
  });

  it("handles directives inside attribute values and tags", () => {
    const t = blade('<input class="a @if($x) b @endif" @disabled($off)>');
    expect(t).toContainEqual(["@if", "keyword"]);
    expect(t).toContainEqual(["@endif", "keyword"]);
    expect(t).toContainEqual(["@disabled", "keyword"]);
    expect(t).toContainEqual(["$off", "variableName"]);
    expect(t.at(-1)).toEqual([">", "angleBracket"]);
  });

  it("knows custom directives only when registered", () => {
    expect(blade("@money($price)")[0][1]).toBeNull();
    expect(blade("@money($price)", ["money"])[0]).toEqual(["@money", "keyword"]);
  });

  it("highlights embedded script and PHP tags", () => {
    const t = blade("<script>\nconst n = {{ $count }};\n</script>\n<?php echo $a; ?>");
    expect(t).toContainEqual(["const", "keyword"]);
    expect(t).toContainEqual(["$count", "variableName"]);
    expect(t).toContainEqual(["script", "tagName"]);
    expect(t).toContainEqual(["<?php", "processingInstruction"]);
    expect(t).toContainEqual(["echo", "keyword"]);
    expect(t.at(-1)).toEqual(["?>", "processingInstruction"]);
  });
});
