/**
 * Highlighting of game engine files (per-keystroke work, so it stays in TypeScript):
 *
 * - Unity: ShaderLab (`.shader`) with HLSL / Cg inside `HLSLPROGRAM` / `CGPROGRAM` blocks,
 *   HLSL include files (`.hlsl`, `.cginc`, `.compute`); `.asmdef`, `.uss` and `.uxml` are
 *   JSON, CSS and XML (bundled file types; the asmdef schema comes from `forge-jsonschema`),
 * - Godot: GDScript (`.gd`), scenes and resources (`.tscn`, `.tres`, `project.godot`,
 *   `.import`) and Godot shaders (`.gdshader`).
 */
import type { StreamParser, StringStream } from "@codemirror/language";
import { registerLanguage } from "@/editor/languages";
import { streamLanguage } from "@/editor/tokens";

const words = (s: string) => new Set(s.split(/\s+/).filter(Boolean));

// ---- C-like shader languages (HLSL, Cg, Godot shaders) ------------------------------------------

interface CLikeSpec {
  keywords: Set<string>;
  types: RegExp;
  builtins: Set<string>;
  atoms?: Set<string>;
  /** Uppercase semantics after `:` (`SV_Target`, `TEXCOORD0`). */
  semantics?: boolean;
  /** Identifiers that are the language's built-in variables (`TIME`, `UV`). */
  constants?: RegExp;
}

export interface CLikeState {
  comment: boolean;
  afterColon: boolean;
}

/** One token of a C-like shader language; null when the stream is at a newline. */
function cLikeToken(spec: CLikeSpec, stream: StringStream, state: CLikeState): string | null {
  if (state.comment) {
    if (stream.skipTo("*/")) {
      stream.match("*/");
      state.comment = false;
    } else stream.skipToEnd();
    return "comment";
  }
  if (stream.eatSpace()) return null;
  if (stream.match("//")) {
    stream.skipToEnd();
    return "comment";
  }
  if (stream.match("/*")) {
    state.comment = true;
    return cLikeToken(spec, stream, state) ?? "comment";
  }
  if (stream.peek() === "#" && !stream.string.slice(0, stream.pos).trim() && stream.match(/^#\s*\w+/)) {
    // `#include "x"` and `#pragma vertex vert`: the directive, the rest as meta too.
    stream.skipToEnd();
    return "meta";
  }
  if (stream.match(/^"(?:[^"\\]|\\.)*"?/)) return "string";
  if (stream.match(/^(?:0[xX][0-9a-fA-F]+[uUlL]*|(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?[fFhHuUlL]?)/)) return "number";
  const id = stream.match(/^[A-Za-z_]\w*/) as RegExpMatchArray | null;
  if (id) {
    const w = id[0];
    const afterColon = state.afterColon;
    state.afterColon = false;
    if (afterColon && spec.semantics && /^[A-Z_][A-Z0-9_]*$/.test(w)) return "atom";
    if (spec.keywords.has(w)) return "keyword";
    if (spec.atoms?.has(w)) return "atom";
    if (spec.types.test(w)) return "type";
    if (spec.constants?.test(w)) return "builtin";
    if (spec.builtins.has(w) && stream.match(/^\s*\(/, false)) return "builtin";
    if (stream.match(/^\s*\(/, false)) return "func";
    return "name";
  }
  if (stream.eat(":")) {
    state.afterColon = true;
    return "operator";
  }
  state.afterColon = false;
  if (stream.match(/^[{}()[\]]/)) return "bracket";
  if (stream.match(/^[-+*/%=<>!&|^~?]+/)) return "operator";
  stream.next();
  return "punct";
}

const HLSL: CLikeSpec = {
  keywords: words(`if else for while do return break continue discard switch case default struct cbuffer tbuffer
    static const uniform in out inout inline typedef register packoffset groupshared extern volatile precise
    nointerpolation linear centroid noperspective sample row_major column_major class interface namespace
    unroll loop branch flatten numthreads`),
  types: /^(?:void|bool|int|uint|dword|half|float|double|fixed|min16float|min10float|min16int|min12int|min16uint)(?:[1-4](?:x[1-4])?)?$|^(?:matrix|vector|string|sampler|sampler1D|sampler2D|sampler3D|samplerCUBE|sampler2D_float|sampler_state|SamplerState|SamplerComparisonState|Texture1D|Texture2D|Texture3D|TextureCube|Texture2DArray|TextureCubeArray|RWTexture1D|RWTexture2D|RWTexture3D|RWTexture2DArray|Buffer|RWBuffer|StructuredBuffer|RWStructuredBuffer|ByteAddressBuffer|RWByteAddressBuffer|AppendStructuredBuffer|ConsumeStructuredBuffer|TEXTURE2D|TEXTURE2D_ARRAY|TEXTURECUBE|TEXTURE3D|SAMPLER|SAMPLER_CMP)$/,
  builtins: words(`abs acos all any asin atan atan2 ceil clamp clip cos cosh cross ddx ddy degrees determinant distance
    dot exp exp2 faceforward floor fmod frac frexp fwidth isfinite isinf isnan ldexp length lerp lit log log10 log2 max
    min modf mul noise normalize pow radians reflect refract round rsqrt saturate sign sin sincos sinh smoothstep sqrt
    step tan tanh tex1D tex2D tex2Dlod tex2Dproj tex3D texCUBE transpose trunc asfloat asint asuint countbits firstbithigh
    reversebits InterlockedAdd GroupMemoryBarrierWithGroupSync UnityObjectToClipPos UnityObjectToWorldNormal
    UnityWorldSpaceViewDir TRANSFORM_TEX UNITY_TRANSFER_FOG UNITY_APPLY_FOG TransformObjectToHClip
    TransformObjectToWorld TransformWorldToHClip TransformObjectToWorldNormal GetVertexPositionInputs
    GetVertexNormalInputs GetMainLight SAMPLE_TEXTURE2D SAMPLE_TEXTURE2D_LOD SAMPLE_TEXTURECUBE LOAD_TEXTURE2D
    CBUFFER_START CBUFFER_END UNITY_DECLARE_TEX2D UNITY_SAMPLE_TEX2D`),
  atoms: words("true false NULL"),
  semantics: true,
};

const GDSHADER: CLikeSpec = {
  keywords: words(`shader_type render_mode uniform varying const if else for while do return break continue discard switch
    case default struct in out inout flat smooth lowp mediump highp instance global group_uniforms void
    stencil_mode`),
  types: /^(?:bool|int|uint|float|[biu]?vec[234]|mat[234]|sampler2D|isampler2D|usampler2D|sampler2DArray|isampler2DArray|usampler2DArray|sampler3D|isampler3D|usampler3D|samplerCube|samplerCubeArray|samplerExternalOES)$/,
  builtins: words(`radians degrees sin cos tan asin acos atan sinh cosh tanh asinh acosh atanh pow exp log exp2 log2 sqrt
    inversesqrt abs sign floor round roundEven trunc ceil fract mod modf min max clamp mix step smoothstep isnan isinf
    length distance dot cross normalize reflect refract faceforward matrixCompMult outerProduct transpose determinant
    inverse lessThan greaterThan equal notEqual any all not texture textureLod texelFetch textureSize dFdx dFdy fwidth
    packHalf2x16 unpackHalf2x16`),
  atoms: words("true false"),
  constants: /^(?:TIME|PI|TAU|E|VERTEX|NORMAL|TANGENT|BINORMAL|UV|UV2|COLOR|ALBEDO|ALPHA|ALPHA_SCISSOR_THRESHOLD|EMISSION|METALLIC|ROUGHNESS|SPECULAR|NORMAL_MAP|NORMAL_MAP_DEPTH|RIM|AO|FRAGCOORD|SCREEN_UV|SCREEN_TEXTURE|TEXTURE|TEXTURE_PIXEL_SIZE|POINT_COORD|POINT_SIZE|LIGHT|LIGHT_COLOR|LIGHT_POSITION|LIGHT_DIRECTION|ATTENUATION|VIEW|CAMERA_POSITION_WORLD|MODEL_MATRIX|VIEW_MATRIX|PROJECTION_MATRIX|INV_VIEW_MATRIX|NODE_POSITION_WORLD|WORLD_VERTEX_COORDS|SKY_COORDS|EYEDIR|POSITION|INSTANCE_ID|INSTANCE_CUSTOM|VERTEX_ID|hint_\w+|source_color|filter_\w+|repeat_\w+)$/,
};

export const hlslParser: StreamParser<CLikeState> = {
  name: "hlsl",
  startState: () => ({ comment: false, afterColon: false }),
  copyState: (s) => ({ ...s }),
  token: (stream, state) => cLikeToken(HLSL, stream, state),
  languageData: { commentTokens: { line: "//", block: { open: "/*", close: "*/" } } },
};

export const gdshaderParser: StreamParser<CLikeState> = {
  name: "gdshader",
  startState: () => ({ comment: false, afterColon: false }),
  copyState: (s) => ({ ...s }),
  token: (stream, state) => cLikeToken(GDSHADER, stream, state),
  languageData: { commentTokens: { line: "//", block: { open: "/*", close: "*/" } } },
};

// ---- ShaderLab ----------------------------------------------------------------------------------

const SHADERLAB_KEYWORDS = words(`Shader Properties SubShader Pass Tags LOD Cull ZWrite ZTest ZClip Blend BlendOp
  AlphaToMask ColorMask Offset Fallback FallBack CustomEditor CustomEditorForRenderPipeline Name UsePass GrabPass
  Stencil Ref ReadMask WriteMask Comp CompBack CompFront Fail ZFail PassFront PassBack FailFront FailBack ZFailFront
  ZFailBack Conservative Lighting Material Fog Category Dependency PackageRequirements HLSLINCLUDE CGINCLUDE
  ENDHLSL ENDCG GLSLINCLUDE ENDGLSL`);
const SHADERLAB_VALUES = words(`On Off Back Front Less Greater LEqual GEqual Equal NotEqual Always Never One Zero
  SrcColor SrcAlpha DstColor DstAlpha OneMinusSrcColor OneMinusSrcAlpha OneMinusDstColor OneMinusDstAlpha Add Sub
  RevSub Min Max Keep Replace IncrSat DecrSat Invert IncrWrap DecrWrap True False`);
const PROPERTY_TYPES = words("Color Range Float Int Integer Vector 2D 3D Cube 2DArray CubeArray Any");

export interface ShaderLabState {
  /** Inside `HLSLPROGRAM` … `ENDHLSL` (or the Cg / GLSL ones). */
  program: boolean;
  comment: boolean;
  hlsl: CLikeState;
}

export const shaderlabParser: StreamParser<ShaderLabState> = {
  name: "shaderlab",
  startState: () => ({ program: false, comment: false, hlsl: { comment: false, afterColon: false } }),
  copyState: (s) => ({ ...s, hlsl: { ...s.hlsl } }),
  token(stream, state) {
    if (state.program) {
      if (!state.hlsl.comment && stream.eatSpace()) return null;
      if (!stream.string.slice(0, stream.pos).trim() && stream.match(/^(?:ENDHLSL|ENDCG|ENDGLSL)\b/)) {
        state.program = false;
        return "keyword";
      }
      return cLikeToken(HLSL, stream, state.hlsl);
    }
    if (state.comment) {
      if (stream.skipTo("*/")) {
        stream.match("*/");
        state.comment = false;
      } else stream.skipToEnd();
      return "comment";
    }
    if (stream.eatSpace()) return null;
    if (stream.match("//")) {
      stream.skipToEnd();
      return "comment";
    }
    if (stream.match("/*")) {
      state.comment = true;
      return "comment";
    }
    if (stream.match(/^(?:HLSLPROGRAM|CGPROGRAM|GLSLPROGRAM|HLSLINCLUDE|CGINCLUDE|GLSLINCLUDE)\b/)) {
      state.program = true;
      return "keyword";
    }
    if (stream.match(/^"(?:[^"\\]|\\.)*"?/)) return "string";
    if (stream.match(/^\[[^\]\n]*\]?/)) return "meta";
    if (stream.match(/^-?(?:\d+\.?\d*|\.\d+)/)) return "number";
    const id = stream.match(/^[A-Za-z_]\w*/) as RegExpMatchArray | null;
    if (id) {
      const w = id[0];
      if (SHADERLAB_KEYWORDS.has(w)) return "keyword";
      if (SHADERLAB_VALUES.has(w)) return "atom";
      if (PROPERTY_TYPES.has(w)) return "type";
      if (w.startsWith("_")) return "property";
      return "name";
    }
    if (stream.match(/^[{}()]/)) return "bracket";
    stream.next();
    return "punct";
  },
  languageData: { commentTokens: { line: "//", block: { open: "/*", close: "*/" } } },
};

// ---- GDScript -----------------------------------------------------------------------------------

const GD_KEYWORDS = words(`if elif else for while match when break continue pass return class class_name extends is
  in as signal func static const enum var await breakpoint preload super assert void`);
const GD_LOGIC = words("and or not");
const GD_ATOMS = words("true false null self PI TAU INF NAN");
const GD_TYPES = /^(?:int|float|bool|String|StringName|NodePath|Vector2i?|Vector3i?|Vector4i?|Rect2i?|Transform2D|Transform3D|Plane|Quaternion|AABB|Basis|Projection|Color|RID|Object|Callable|Signal|Dictionary|Array|Packed(?:Byte|Int32|Int64|Float32|Float64|String|Vector2|Vector3|Vector4|Color)Array|Variant)$/;
const GD_BUILTINS = words(`print printt prints print_rich push_error push_warning len range str int float bool load
  preload instance_from_id is_instance_valid typeof type_string weakref randi randf randf_range randi_range seed abs
  clamp lerp min max sign floor ceil round sqrt pow sin cos tan atan2 deg_to_rad rad_to_deg get_node get_tree
  emit_signal connect queue_free add_child remove_child`);

export interface GdState {
  /** Inside a `"""` / `'''` string. */
  multiline: string | null;
  /** The previous word was `func` / `class_name` / `signal` / `class`. */
  defines: string | null;
}

function gdToken(stream: StringStream, state: GdState): string | null {
  if (state.multiline) {
    if (stream.skipTo(state.multiline)) {
      stream.match(state.multiline);
      state.multiline = null;
    } else stream.skipToEnd();
    return "string";
  }
  if (stream.eatSpace()) return null;
  if (stream.peek() === "#") {
    stream.skipToEnd();
    return "comment";
  }
  const triple = stream.match(/^(?:"""|''')/) as RegExpMatchArray | null;
  if (triple) {
    state.multiline = triple[0];
    return gdToken(stream, state) ?? "string";
  }
  if (stream.match(/^[&^]?"(?:[^"\\]|\\.)*"?/) || stream.match(/^[&^]?'(?:[^'\\]|\\.)*'?/)) {
    const t = stream.current();
    return t.startsWith("&") || t.startsWith("^") ? "special" : "string";
  }
  if (stream.match(/^[$%](?:"[^"]*"|[\w/]+)/)) return "special";
  if (stream.match(/^@\w+/)) return "meta";
  if (stream.match(/^(?:0x[0-9a-fA-F_]+|0b[01_]+|(?:\d[\d_]*\.?[\d_]*|\.\d[\d_]*)(?:[eE][-+]?\d+)?)/)) return "number";
  const id = stream.match(/^[A-Za-z_]\w*/) as RegExpMatchArray | null;
  if (id) {
    const w = id[0];
    const defines = state.defines;
    state.defines = null;
    if (defines === "func" || defines === "signal") return "func";
    if (defines === "class_name" || defines === "class") return "type";
    if (GD_KEYWORDS.has(w)) {
      if (w === "func" || w === "class_name" || w === "signal" || w === "class") state.defines = w;
      return w === "if" || w === "elif" || w === "else" || w === "for" || w === "while" || w === "match" || w === "return" || w === "break" || w === "continue" ? "control" : "keyword";
    }
    if (GD_LOGIC.has(w)) return "keyword";
    if (GD_ATOMS.has(w)) return "atom";
    if (GD_TYPES.test(w)) return "type";
    if (GD_BUILTINS.has(w) && stream.match(/^\s*\(/, false)) return "builtin";
    if (/^[A-Z][A-Za-z0-9]*$/.test(w) && /[a-z]/.test(w)) return "type";
    if (stream.match(/^\s*\(/, false)) return "func";
    return "name";
  }
  if (stream.match(/^->|^:=|^[-+*/%=<>!&|^~]+/)) return "operator";
  if (stream.match(/^[{}()[\]]/)) return "bracket";
  stream.next();
  return "punct";
}

export const gdscriptParser: StreamParser<GdState> = {
  name: "gdscript",
  startState: () => ({ multiline: null, defines: null }),
  copyState: (s) => ({ ...s }),
  token: gdToken,
  languageData: { commentTokens: { line: "#" }, indentOnInput: /^\s*(?:elif|else)\b/ },
};

// ---- Godot scenes and resources -----------------------------------------------------------------

export interface GodotResourceState {
  /** Inside a `[section …]` header. */
  header: boolean;
  /** After the first word of the header (its attributes follow). */
  named: boolean;
  /** After `=` on a `key = value` line. */
  value: boolean;
}

export const godotResourceParser: StreamParser<GodotResourceState> = {
  name: "godot-resource",
  startState: () => ({ header: false, named: false, value: false }),
  copyState: (s) => ({ ...s }),
  token(stream, state) {
    if (stream.sol() && !state.header) state.value = false;
    if (stream.eatSpace()) return null;
    if (!state.header && stream.sol() && (stream.peek() === ";" || stream.peek() === "#")) {
      stream.skipToEnd();
      return "comment";
    }
    if (!state.value && stream.sol() && stream.eat("[")) {
      state.header = true;
      state.named = false;
      return "bracket";
    }
    if (state.header) {
      if (stream.eat("]")) {
        state.header = false;
        return "bracket";
      }
      if (!state.named) {
        stream.match(/^[^\s\]]+/);
        state.named = true;
        return "keyword";
      }
      if (stream.match(/^[\w/]+(?==)/)) return "option";
      if (stream.eat("=")) return "operator";
    } else if (!state.value) {
      if (stream.match(/^[^=\s][^=]*?(?=\s*=)/)) return "property";
      if (stream.eat("=")) {
        state.value = true;
        return "operator";
      }
    }
    if (stream.match(/^[&^]?"(?:[^"\\]|\\.)*"?/)) return stream.current().startsWith("&") ? "special" : "string";
    if (stream.match(/^-?(?:\d+\.?\d*|\.\d+)(?:e[-+]?\d+)?/i)) return "number";
    if (stream.match(/^(?:true|false|null|inf|nan)\b/)) return "atom";
    if (stream.match(/^(?:ExtResource|SubResource|Resource|NodePath|StringName|Object)\b/)) return "builtin";
    if (stream.match(/^[A-Z]\w*(?=\()/)) return "type";
    if (stream.match(/^[A-Za-z_][\w/]*/)) return state.value ? "name" : "property";
    if (stream.match(/^[{}()[\]]/)) return "bracket";
    stream.next();
    return "punct";
  },
  languageData: { commentTokens: { line: ";" } },
};

registerLanguage({ id: "shaderlab", load: () => streamLanguage(shaderlabParser) });
registerLanguage({ id: "hlsl", load: () => streamLanguage(hlslParser) });
registerLanguage({ id: "gdscript", load: () => streamLanguage(gdscriptParser) });
registerLanguage({ id: "godot-resource", load: () => streamLanguage(godotResourceParser) });
registerLanguage({ id: "gdshader", load: () => streamLanguage(gdshaderParser) });
