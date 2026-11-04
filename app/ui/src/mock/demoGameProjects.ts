/**
 * Game engine demos of DotNetForge for browser mode:
 *
 * - `?project=unity`: `Shooter`, a Unity 2022.3 project (scripts with an assembly
 *   definition, Edit Mode tests, a toon shader with an HLSL include, UI Toolkit files) with
 *   the Forge editor bridge connected to a simulated editor (`gamedevMock.ts`);
 *   `&bridge=none` starts without the bridge package.
 * - `?project=godot`: `Dodge the Creeps`, a Godot 4.3 project with GDScript, a C# script,
 *   scenes, a Godot shader and a GdUnit4 suite; `&editor=closed` starts without a running
 *   Godot editor (no GDScript language server).
 */
import type { ProjectInfo } from "@/ipc/project";
import type { RunConfigurations } from "@/ipc/run";

const param = (key: string) => (typeof location !== "undefined" ? new URLSearchParams(location.search).get(key) : null);

export const isUnityDemo = (): boolean => param("project") === "unity";
export const isGodotDemo = (): boolean => param("project") === "godot";
export const isGameDemo = (): boolean => isUnityDemo() || isGodotDemo();
/** `?project=unity&bridge=none`: the bridge package is not installed. */
export const unityWithoutBridge = (): boolean => isUnityDemo() && param("bridge") === "none";
/** `?project=godot&editor=closed`: no Godot editor runs. */
export const godotEditorClosed = (): boolean => isGodotDemo() && param("editor") === "closed";

export const unityDemoName = "Shooter";
export const godotDemoName = "dodge-the-creeps";

const unity = { id: "unity", name: "Unity", version: "2022.3.10f1", language: "csharp" };
export const unityDemoInfo: ProjectInfo = {
  languages: ["csharp"],
  frameworks: [unity],
  packs: ["dotnetforge"],
  tools: ["git", "dotnet"],
  primary: unity,
};

const godot = { id: "godot", name: "Godot", version: "4.3", language: "gdscript" };
export const godotDemoInfo: ProjectInfo = {
  languages: ["gdscript", "csharp"],
  frameworks: [godot],
  packs: ["dotnetforge"],
  tools: ["git", "dotnet"],
  primary: godot,
};

export const unityDemoStartFile = "Assets/Scripts/PlayerController.cs";
export const godotDemoStartFile = "player.gd";

export const UNITY_CONFIGURATIONS: RunConfigurations = {
  selected: "Edit Mode Tests",
  configurations: [
    { name: "Edit Mode Tests", type: "unity-test", storeAsProjectFile: true, options: { scope: "all", platform: "EditMode" } },
    { name: "PlayerTests", type: "unity-test", options: { scope: "class", class: "Shooter.Tests.PlayerTests" } },
  ],
};

export const GODOT_CONFIGURATIONS: RunConfigurations = {
  selected: "Dodge the Creeps",
  configurations: [
    { name: "Dodge the Creeps", type: "godot", storeAsProjectFile: true, options: { target: "project" } },
    { name: "player", type: "godot", options: { target: "scene", scene: "player.tscn" } },
    { name: "GdUnit4 Tests", type: "gdunit4", storeAsProjectFile: true, options: { scope: "all" } },
  ],
};

const bridgeFiles: Record<string, string> = {
  "Packages/dev.forge.unity/package.json": `{
  "name": "dev.forge.unity",
  "displayName": "Forge Editor Bridge",
  "version": "1.0.0",
  "unity": "2019.4",
  "license": "MIT"
}
`,
};

export const unityDemoFiles: Record<string, string> = {
  ".gitignore": `/[Ll]ibrary/
/[Tt]emp/
/[Oo]bj/
/[Bb]uild/
/[Bb]uilds/
/[Ll]ogs/
/[Uu]ser[Ss]ettings/
*.csproj
*.sln
.vs/
`,
  "ProjectSettings/ProjectVersion.txt": `m_EditorVersion: 2022.3.10f1
m_EditorVersionWithRevision: 2022.3.10f1 (ff3792e53c62)
`,
  "Packages/manifest.json": `{
  "dependencies": {
    "com.unity.ide.rider": "3.0.24",
    "com.unity.ide.visualstudio": "2.0.20",
    "com.unity.inputsystem": "1.7.0",
    "com.unity.test-framework": "1.1.33",
    "com.unity.textmeshpro": "3.0.6",
    "com.unity.ugui": "1.0.0",
    "com.unity.modules.physics": "1.0.0"
  }
}
`,
  ...(unityWithoutBridge() ? {} : bridgeFiles),
  "Shooter.sln": `Microsoft Visual Studio Solution File, Format Version 11.00
# Visual Studio 2010
Project("{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}") = "Shooter.Game", "Shooter.Game.csproj", "{6A1F3C1E-0D2B-4C3A-9E1F-0B2C3D4E5F60}"
EndProject
Project("{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}") = "Shooter.Tests.EditMode", "Shooter.Tests.EditMode.csproj", "{7B2A4D2F-1E3C-4D4B-AF20-1C3D4E5F6071}"
EndProject
Global
	GlobalSection(SolutionConfigurationPlatforms) = preSolution
		Debug|Any CPU = Debug|Any CPU
	EndGlobalSection
EndGlobal
`,
  "Shooter.Game.csproj": `<?xml version="1.0" encoding="utf-8"?>
<Project ToolsVersion="4.0" DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <LangVersion>9.0</LangVersion>
    <OutputType>Library</OutputType>
    <AssemblyName>Shooter.Game</AssemblyName>
    <TargetFrameworkVersion>v4.7.1</TargetFrameworkVersion>
  </PropertyGroup>
  <ItemGroup>
    <Compile Include="Assets/Scripts/PlayerController.cs" />
    <Compile Include="Assets/Scripts/Enemy.cs" />
  </ItemGroup>
</Project>
`,
  "Shooter.Tests.EditMode.csproj": `<?xml version="1.0" encoding="utf-8"?>
<Project ToolsVersion="4.0" DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <PropertyGroup>
    <AssemblyName>Shooter.Tests.EditMode</AssemblyName>
  </PropertyGroup>
  <ItemGroup>
    <Compile Include="Assets/Tests/EditMode/PlayerTests.cs" />
  </ItemGroup>
</Project>
`,
  "Assets/Scripts/Shooter.Game.asmdef": `{
    "name": "Shooter.Game",
    "rootNamespace": "Shooter",
    "references": [
        "Unity.InputSystem",
        "Unity.TextMeshPro"
    ],
    "includePlatforms": [],
    "excludePlatforms": [],
    "allowUnsafeCode": false,
    "autoReferenced": true,
    "defineConstraints": [],
    "versionDefines": [
        {
            "name": "com.unity.inputsystem",
            "expression": "1.0.0",
            "define": "HAS_INPUT_SYSTEM"
        }
    ],
    "noEngineReferences": false
}
`,
  "Assets/Scripts/PlayerController.cs": `using UnityEngine;

namespace Shooter
{
    /// <summary>Moves the ship and fires bullets.</summary>
    [RequireComponent(typeof(Rigidbody))]
    public class PlayerController : MonoBehaviour
    {
        [SerializeField] private float speed = 8f;
        [SerializeField] private float jumpVelocity = 5f;
        [SerializeField] private Bullet bulletPrefab;

        public int Health { get; private set; } = 5;

        private Rigidbody body;

        private void Awake()
        {
            body = GetComponent<Rigidbody>();
        }

        private void Update()
        {
            var input = new Vector3(Input.GetAxis("Horizontal"), 0f, Input.GetAxis("Vertical"));
            body.velocity = new Vector3(input.x * speed, body.velocity.y, input.z * speed);
            if (Input.GetButtonDown("Jump"))
                Jump();
            if (Input.GetButtonDown("Fire1"))
                Instantiate(bulletPrefab, transform.position, transform.rotation);
        }

        public void Jump()
        {
            body.velocity = new Vector3(body.velocity.x, jumpVelocity, body.velocity.z);
        }

        public void TakeDamage(int amount)
        {
            if (amount <= 0)
                throw new System.ArgumentOutOfRangeException(nameof(amount), "Damage must be positive");
            Health = Mathf.Max(0, Health - amount);
            if (Health == 0)
                Debug.Log($"{name} was destroyed");
        }
    }
}
`,
  "Assets/Scripts/Enemy.cs": `using UnityEngine;

namespace Shooter
{
    public class Enemy : MonoBehaviour
    {
        [SerializeField] private float speed = 2f;
        private Transform target;

        private void Start()
        {
            target = FindObjectOfType<PlayerController>().transform;
        }

        private void Update()
        {
            transform.position = Vector3.MoveTowards(transform.position, target.position, speed * Time.deltaTime);
        }
    }
}
`,
  "Assets/Scripts/Bullet.cs": `using UnityEngine;

namespace Shooter
{
    public class Bullet : MonoBehaviour
    {
        public float Speed = 20f;

        private void Update() => transform.Translate(Vector3.forward * (Speed * Time.deltaTime));
    }
}
`,
  "Assets/Tests/EditMode/Shooter.Tests.EditMode.asmdef": `{
    "name": "Shooter.Tests.EditMode",
    "rootNamespace": "Shooter.Tests",
    "references": [
        "Shooter.Game",
        "UnityEngine.TestRunner",
        "UnityEditor.TestRunner"
    ],
    "includePlatforms": [
        "Editor"
    ],
    "overrideReferences": true,
    "precompiledReferences": [
        "nunit.framework.dll"
    ],
    "autoReferenced": false,
    "defineConstraints": [
        "UNITY_INCLUDE_TESTS"
    ]
}
`,
  "Assets/Tests/EditMode/PlayerTests.cs": `using System.Collections;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;

namespace Shooter.Tests
{
    public class PlayerTests
    {
        private PlayerController player;

        [SetUp]
        public void CreatePlayer()
        {
            player = new GameObject("Player").AddComponent<Rigidbody>().gameObject.AddComponent<PlayerController>();
        }

        [Test]
        public void Jump_SetsVelocity()
        {
            player.Jump();
            Assert.That(player.GetComponent<Rigidbody>().velocity.y, Is.EqualTo(5f));
        }

        [Test]
        public void Health_Decreases()
        {
            player.TakeDamage(2);
            Assert.AreEqual(5, player.Health);
        }

        [TestCase(10)]
        [TestCase(-1)]
        public void Damage(int amount)
        {
            player.TakeDamage(amount);
            Assert.That(player.Health, Is.GreaterThanOrEqualTo(0));
        }

        [Test, Ignore("Waiting for the new spawn system")]
        public void Respawn()
        {
        }

        [UnityTest]
        public IEnumerator FallsWithGravity()
        {
            var y = player.transform.position.y;
            yield return null;
            Assert.LessOrEqual(player.transform.position.y, y);
        }
    }
}
`,
  "Assets/Shaders/Toon.shader": `Shader "Shooter/Toon"
{
    Properties
    {
        [MainTexture] _BaseMap ("Base Map", 2D) = "white" {}
        [HDR] _Color ("Color", Color) = (1, 1, 1, 1)
        _Steps ("Shading Steps", Range(1, 8)) = 3
    }
    SubShader
    {
        Tags { "RenderType" = "Opaque" "RenderPipeline" = "UniversalPipeline" }
        LOD 100
        Cull Back
        ZWrite On

        Pass
        {
            Name "ForwardLit"
            HLSLPROGRAM
            #pragma vertex vert
            #pragma fragment frag
            #include "ToonLighting.hlsl"

            struct Attributes
            {
                float4 positionOS : POSITION;
                float3 normalOS : NORMAL;
                float2 uv : TEXCOORD0;
            };

            struct Varyings
            {
                float4 positionCS : SV_POSITION;
                float3 normalWS : TEXCOORD1;
                float2 uv : TEXCOORD0;
            };

            Varyings vert(Attributes input)
            {
                Varyings output;
                output.positionCS = TransformObjectToHClip(input.positionOS.xyz);
                output.normalWS = TransformObjectToWorldNormal(input.normalOS);
                output.uv = input.uv;
                return output;
            }

            half4 frag(Varyings input) : SV_Target
            {
                half3 albedo = SAMPLE_TEXTURE2D(_BaseMap, sampler_BaseMap, input.uv).rgb * _Color.rgb;
                return half4(albedo * ToonRamp(normalize(input.normalWS), _Steps), 1.0);
            }
            ENDHLSL
        }
    }
    Fallback "Universal Render Pipeline/Lit"
}
`,
  "Assets/Shaders/ToonLighting.hlsl": `#ifndef SHOOTER_TOON_LIGHTING
#define SHOOTER_TOON_LIGHTING

#include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Lighting.hlsl"

TEXTURE2D(_BaseMap);
SAMPLER(sampler_BaseMap);

CBUFFER_START(UnityPerMaterial)
    float4 _Color;
    float _Steps;
CBUFFER_END

/* Quantized diffuse lighting: the classic cel look. */
half ToonRamp(float3 normalWS, float steps)
{
    Light light = GetMainLight();
    half ndotl = saturate(dot(normalWS, light.direction));
    return floor(ndotl * steps) / steps;
}

#endif
`,
  "Assets/UI/MainMenu.uss": `.menu {
    flex-grow: 1;
    align-items: center;
    justify-content: center;
    background-color: rgba(10, 12, 20, 0.85);
}

.menu__button {
    width: 240px;
    margin: 6px;
    font-size: 22px;
    -unity-font-style: bold;
}
`,
  "Assets/UI/MainMenu.uxml": `<ui:UXML xmlns:ui="UnityEngine.UIElements" xmlns:uie="UnityEditor.UIElements">
    <Style src="MainMenu.uss" />
    <ui:VisualElement class="menu">
        <ui:Label text="SHOOTER" name="title" />
        <ui:Button text="Play" name="play" class="menu__button" />
        <ui:Button text="Quit" name="quit" class="menu__button" />
    </ui:VisualElement>
</ui:UXML>
`,
};

export const godotDemoFiles: Record<string, string> = {
  ".gitignore": `# Godot 4+ specific ignores
.godot/
/android/
# .NET
.mono/
data_*/
mono_crash.*.json
`,
  "project.godot": `; Engine configuration file.
; It's best edited using the editor UI and not directly,
; since the parameters that go here are not all obvious.
;
; Format:
;   [section] ; section goes between []
;   param=value ; assign values to parameters

config_version=5

[application]

config/name="Dodge the Creeps"
config/description="Dodge the creeps as long as you can."
run/main_scene="res://main.tscn"
config/features=PackedStringArray("4.3", "C#", "Forward Plus")
config/icon="res://icon.svg"

[autoload]

Score="*res://autoload/score.gd"

[display]

window/size/viewport_width=480
window/size/viewport_height=720

[dotnet]

project/assembly_name="Dodge the Creeps"

[editor_plugins]

enabled=PackedStringArray("res://addons/gdUnit4/plugin.cfg")
`,
  "main.tscn": `[gd_scene load_steps=5 format=3 uid="uid://h4bqmqyhcjrv"]

[ext_resource type="Script" path="res://main.gd" id="1_jw4q1"]
[ext_resource type="PackedScene" uid="uid://rkdnhqgf2hpj" path="res://player.tscn" id="2_bv1jx"]
[ext_resource type="PackedScene" uid="uid://dlk6ulp4bms5" path="res://hud.tscn" id="3_x1vqy"]

[sub_resource type="Curve2D" id="Curve2D_1"]
_data = {
"points": PackedVector2Array(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 480, 0, 0, 0, 0, 0, 480, 720)
}
point_count = 3

[node name="Main" type="Node"]
script = ExtResource("1_jw4q1")

[node name="Player" parent="." instance=ExtResource("2_bv1jx")]

[node name="MobTimer" type="Timer" parent="."]
wait_time = 0.5

[node name="StartPosition" type="Marker2D" parent="."]
position = Vector2(240, 450)

[node name="MobPath" type="Path2D" parent="."]
curve = SubResource("Curve2D_1")

[node name="HUD" parent="." instance=ExtResource("3_x1vqy")]

[connection signal="hit" from="Player" to="." method="game_over"]
[connection signal="timeout" from="MobTimer" to="." method="_on_mob_timer_timeout"]
`,
  "player.tscn": `[gd_scene load_steps=3 format=3 uid="uid://rkdnhqgf2hpj"]

[ext_resource type="Script" path="res://player.gd" id="1_f2c3d"]

[sub_resource type="CapsuleShape2D" id="CapsuleShape2D_1"]
radius = 27.0
height = 68.0

[node name="Player" type="Area2D"]
z_index = 10
script = ExtResource("1_f2c3d")
speed = 400

[node name="AnimatedSprite2D" type="AnimatedSprite2D" parent="."]
scale = Vector2(0.5, 0.5)
animation = &"walk"

[node name="CollisionShape2D" type="CollisionShape2D" parent="."]
shape = SubResource("CapsuleShape2D_1")
`,
  "hud.tscn": `[gd_scene load_steps=2 format=3 uid="uid://dlk6ulp4bms5"]

[ext_resource type="Script" path="res://Hud.cs" id="1_hud"]

[node name="HUD" type="CanvasLayer"]
script = ExtResource("1_hud")

[node name="ScoreLabel" type="Label" parent="."]
offset_right = 480.0
offset_bottom = 78.0
text = "0"
horizontal_alignment = 1
`,
  "player.gd": `class_name Player
extends Area2D
## The player's ship: moves with the arrow keys and dies when a creep hits it.

signal hit

@export var speed := 400 ## How fast the player moves (pixels/sec).
var screen_size: Vector2
@onready var sprite: AnimatedSprite2D = $AnimatedSprite2D


func _ready() -> void:
	screen_size = get_viewport_rect().size
	hide()


func _process(delta: float) -> void:
	var velocity := Vector2.ZERO
	if Input.is_action_pressed(&"move_right"):
		velocity.x += 1
	if Input.is_action_pressed(&"move_left"):
		velocity.x -= 1
	if velocity.length() > 0:
		velocity = velocity.normalized() * speed
		sprite.play()
	else:
		sprite.stop()
	position += velocity * delta
	position = position.clamp(Vector2.ZERO, screen_size)


func start(pos: Vector2) -> void:
	position = pos
	show()
	$CollisionShape2D.disabled = false


func _on_body_entered(_body: Node2D) -> void:
	hide()
	hit.emit()
	$CollisionShape2D.set_deferred(&"disabled", true)
`,
  "main.gd": `extends Node

@export var mob_scene: PackedScene
var score: int


func new_game() -> void:
	score = 0
	$Player.start($StartPosition.position)
	$StartTimer.start()
	$HUD.call("UpdateScore", score)


func game_over() -> void:
	$ScoreTimer.stop()
	$MobTimer.stop()
	Score.record(score)


func _on_mob_timer_timeout() -> void:
	var mob: RigidBody2D = mob_scene.instantiate()
	var location: PathFollow2D = $MobPath/MobSpawnLocation
	location.progress_ratio = randf()
	mob.position = location.position
	add_child(mob)
`,
  "autoload/score.gd": `extends Node
## Keeps the best score of the session.

var best := 0


func record(score: int) -> void:
	best = max(best, score)
`,
  "Hud.cs": `using Godot;

public partial class Hud : CanvasLayer
{
    [Signal]
    public delegate void StartGameEventHandler();

    public void UpdateScore(int score)
    {
        GetNode<Label>("ScoreLabel").Text = score.ToString();
    }

    public void ShowMessage(string text)
    {
        var message = GetNode<Label>("Message");
        message.Text = text;
        message.Show();
    }
}
`,
  "Dodge the Creeps.csproj": `<Project Sdk="Godot.NET.Sdk/4.3.0">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <EnableDynamicLoading>true</EnableDynamicLoading>
    <RootNamespace>DodgeTheCreeps</RootNamespace>
  </PropertyGroup>
</Project>
`,
  "Dodge the Creeps.sln": `Microsoft Visual Studio Solution File, Format Version 12.00
# Visual Studio 2012
Project("{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}") = "Dodge the Creeps", "Dodge the Creeps.csproj", "{0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9}"
EndProject
Global
	GlobalSection(SolutionConfigurationPlatforms) = preSolution
		Debug|Any CPU = Debug|Any CPU
		ExportDebug|Any CPU = ExportDebug|Any CPU
		ExportRelease|Any CPU = ExportRelease|Any CPU
	EndGlobalSection
EndGlobal
`,
  "shaders/hit_flash.gdshader": `shader_type canvas_item;

uniform vec4 flash_color : source_color = vec4(1.0);
uniform float flash_amount : hint_range(0.0, 1.0) = 0.0;

void fragment() {
	vec4 base = texture(TEXTURE, UV);
	// Blend toward the flash color, keeping the alpha.
	COLOR = vec4(mix(base.rgb, flash_color.rgb, flash_amount), base.a);
}
`,
  "test/player_test.gd": `# GdUnit generated TestSuite
class_name PlayerTest
extends GdUnitTestSuite
@warning_ignore("unused_parameter")
@warning_ignore("return_value_discarded")

const __source = "res://player.gd"


func test_start_shows_the_player() -> void:
	var player: Player = auto_free(load(__source).new())
	player.start(Vector2(10, 20))
	assert_bool(player.visible).is_true()
	assert_vector(player.position).is_equal(Vector2(10, 20))


func test_speed_default() -> void:
	var player: Player = auto_free(load(__source).new())
	assert_int(player.speed).is_equal(450)
`,
  "addons/gdUnit4/plugin.cfg": `[plugin]

name="gdUnit4"
description="Unit Testing Framework for Godot Scripts"
author="Mike Schulze"
version="4.4.0"
script="plugin.gd"
`,
  "icon.svg": `<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128"><rect width="124" height="124" x="2" y="2" fill="#363d52" rx="14"/></svg>
`,
};
