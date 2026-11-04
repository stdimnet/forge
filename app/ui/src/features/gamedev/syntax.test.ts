import { describe, expect, it } from "vitest";
import { styleOf, tokenize } from "@/features/languages/testkit";
import { gdscriptParser, gdshaderParser, godotResourceParser, hlslParser, shaderlabParser } from "./syntax";

describe("ShaderLab and HLSL", () => {
  const shader = `Shader "Custom/Toon"
{
    Properties
    {
        [HDR] _Color ("Color", Color) = (1, 1, 1, 1)
        _Steps ("Steps", Range(1, 8)) = 3
    }
    SubShader
    {
        Tags { "RenderType"="Opaque" }
        Cull Back
        Pass
        {
            HLSLPROGRAM
            #pragma vertex vert
            float4 _Color;
            float4 vert(float4 pos : POSITION) : SV_POSITION
            {
                return mul(UNITY_MATRIX_MVP, pos); // clip space
            }
            ENDHLSL
        }
    }
}`;

  it("colors ShaderLab and the HLSL program inside it", () => {
    const lines = tokenize(shaderlabParser, shader);
    expect(styleOf(lines[0], "Shader")).toBe("keyword");
    expect(styleOf(lines[0], '"Custom/Toon"')).toBe("string");
    expect(styleOf(lines[4], "[HDR]")).toBe("meta");
    expect(styleOf(lines[4], "_Color")).toBe("property");
    expect(styleOf(lines[4], "Color")).toBe("type");
    expect(styleOf(lines[10], "Back")).toBe("atom");
    expect(styleOf(lines[13], "HLSLPROGRAM")).toBe("keyword");
    expect(lines[14][0][1]).toBe("meta");
    expect(styleOf(lines[15], "float4")).toBe("type");
    expect(styleOf(lines[16], "POSITION")).toBe("atom");
    expect(styleOf(lines[16], "SV_POSITION")).toBe("atom");
    expect(styleOf(lines[16], "vert")).toBe("func");
    expect(styleOf(lines[18], "mul")).toBe("builtin");
    expect(styleOf(lines[18], "// clip space")).toBe("comment");
    expect(styleOf(lines[20], "ENDHLSL")).toBe("keyword");
    expect(styleOf(lines[22], "}")).toBe("bracket");
  });

  it("keeps block comments across lines in HLSL files", () => {
    const lines = tokenize(hlslParser, "/* a\n b */ half3 c = saturate(x);\n#include \"Common.cginc\"");
    expect(lines[0][0]).toEqual(["/* a", "comment"]);
    expect(lines[1][0]).toEqual([" b */", "comment"]);
    expect(styleOf(lines[1], "half3")).toBe("type");
    expect(styleOf(lines[1], "saturate")).toBe("builtin");
    expect(lines[2][0][1]).toBe("meta");
  });
});

describe("GDScript", () => {
  const script = `class_name Player
extends CharacterBody2D
## The player.

signal hit(damage: int)
@export var speed := 400.0
@onready var sprite: Sprite2D = $AnimatedSprite2D

func _physics_process(delta: float) -> void:
\tif Input.is_action_pressed(&"move_left"):
\t\tvelocity.x = -speed
\tvar doc = """multi
line"""
\tprint(Vector2.ZERO, null)`;

  it("colors declarations, annotations, node paths and string names", () => {
    const lines = tokenize(gdscriptParser, script);
    expect(styleOf(lines[0], "class_name")).toBe("keyword");
    expect(styleOf(lines[0], "Player")).toBe("type");
    expect(styleOf(lines[1], "CharacterBody2D")).toBe("type");
    expect(lines[2][0][1]).toBe("comment");
    expect(styleOf(lines[4], "hit")).toBe("func");
    expect(styleOf(lines[4], "int")).toBe("type");
    expect(styleOf(lines[5], "@export")).toBe("meta");
    expect(styleOf(lines[5], ":=")).toBe("operator");
    expect(styleOf(lines[5], "400.0")).toBe("number");
    expect(styleOf(lines[6], "$AnimatedSprite2D")).toBe("special");
    expect(styleOf(lines[8], "_physics_process")).toBe("func");
    expect(styleOf(lines[8], "->")).toBe("operator");
    expect(styleOf(lines[9], "if")).toBe("control");
    expect(styleOf(lines[9], '&"move_left"')).toBe("special");
    expect(lines[12][0]).toEqual(['line"""', "string"]);
    expect(styleOf(lines[13], "print")).toBe("builtin");
    expect(styleOf(lines[13], "null")).toBe("atom");
  });
});

describe("Godot resources and shaders", () => {
  it("colors scene headers, keys and constructors", () => {
    const tscn = `[gd_scene load_steps=3 format=3 uid="uid://b1"]

[ext_resource type="Script" path="res://player.gd" id="1_x"]

[node name="Player" type="CharacterBody2D"]
position = Vector2(240, 450)
script = ExtResource("1_x")
; a comment`;
    const lines = tokenize(godotResourceParser, tscn);
    expect(lines[0][0]).toEqual(["[", "bracket"]);
    expect(styleOf(lines[0], "gd_scene")).toBe("keyword");
    expect(styleOf(lines[0], "load_steps")).toBe("option");
    expect(styleOf(lines[0], "3")).toBe("number");
    expect(styleOf(lines[2], '"res://player.gd"')).toBe("string");
    expect(styleOf(lines[5], "position")).toBe("property");
    expect(styleOf(lines[5], "Vector2")).toBe("type");
    expect(styleOf(lines[6], "ExtResource")).toBe("builtin");
    expect(lines[7][0][1]).toBe("comment");
  });

  it("colors Godot shaders", () => {
    const lines = tokenize(gdshaderParser, "shader_type canvas_item;\nuniform vec4 tint : source_color;\nvoid fragment() { COLOR = texture(TEXTURE, UV) * tint; }");
    expect(styleOf(lines[0], "shader_type")).toBe("keyword");
    expect(styleOf(lines[1], "vec4")).toBe("type");
    expect(styleOf(lines[1], "source_color")).toBe("builtin");
    expect(styleOf(lines[2], "COLOR")).toBe("builtin");
    expect(styleOf(lines[2], "texture")).toBe("builtin");
  });
});
