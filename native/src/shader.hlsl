// The card's background, ported from the shader the earlier versions used.
// Compiled at startup with D3DCompile (d3dcompiler_47.dll ships with Windows).
//
// Fullscreen triangle; `g_resolution` is the render target size in pixels.

cbuffer Params : register(b0)
{
    float g_time;
    float2 g_resolution;
    float g_mode; // 0 = aurora, 1 = ocean
};

struct VSOut
{
    float4 pos : SV_Position;
};

VSOut VSMain(uint id : SV_VertexID)
{
    VSOut o;
    // Clockwise in clip space: Direct3D's default front face.
    float2 p = float2(id == 2 ? 3.0 : -1.0, id == 1 ? 3.0 : -1.0);
    o.pos = float4(p, 0.0, 1.0);
    return o;
}

float hash21(float2 p)
{
    float2 q = frac(p * float2(123.34, 456.21));
    q += dot(q, q + 45.32);
    return frac(q.x * q.y);
}

float vnoise(float2 p)
{
    float2 i = floor(p);
    float2 f = frac(p);
    float2 w = f * f * (3.0 - 2.0 * f);
    float a = hash21(i);
    float b = hash21(i + float2(1.0, 0.0));
    float c = hash21(i + float2(0.0, 1.0));
    float d = hash21(i + float2(1.0, 1.0));
    return lerp(lerp(a, b, w.x), lerp(c, d, w.x), w.y);
}

float fbm(float2 p)
{
    float v = 0.0;
    float a = 0.5;
    float2 q = p;
    for (int i = 0; i < 5; i++)
    {
        v += a * vnoise(q);
        q = q * 2.03 + float2(1.7, 9.2);
        a *= 0.5;
    }
    return v;
}

float3 ocean_col(float2 uv, float t)
{
    const float horizon = 0.50;
    float aspect = g_resolution.x / max(g_resolution.y, 1.0);
    float2 sun = float2(0.64, horizon + 0.045);

    float3 sky_low = float3(1.00, 0.68, 0.38);
    float3 sky_mid = float3(0.72, 0.70, 0.70);
    float3 sky_top = float3(0.34, 0.40, 0.47);
    float h = saturate(uv.y / horizon);
    float3 sky = lerp(sky_low, sky_mid, smoothstep(0.0, 0.55, h));
    sky = lerp(sky, sky_top, smoothstep(0.45, 1.0, h));

    float2 sp = float2((uv.x - sun.x) * aspect, uv.y - sun.y);
    float sd = length(sp);
    float glow = exp(-sd * 4.2);
    float disk = 1.0 - smoothstep(0.034, 0.050, sd);

    float2 cuv = float2(uv.x * 2.1 + t * 0.008, uv.y * 3.6 + t * 0.003);
    float2 warp = float2(fbm(cuv * 0.65), fbm(cuv * 0.65 + float2(4.7, 1.3)));
    float c1 = fbm(cuv + warp * 0.55);
    float c2 = fbm(cuv * 2.2 + warp * 0.7 + float2(5.3, 2.1));
    float cdet = c1 * 0.65 + c2 * 0.35;
    float cloud = smoothstep(0.38, 0.62, cdet) * smoothstep(horizon + 0.01, horizon + 0.18, uv.y);
    float lit = saturate(glow * 0.8 + (uv.y - horizon) * 1.2 + cdet * 0.2);
    float3 cloud_col = lerp(float3(0.18, 0.20, 0.27), float3(1.00, 0.78, 0.54), lit);
    cloud_col *= 0.68 + 0.60 * cdet;
    sky = lerp(sky, cloud_col, cloud * 0.92);
    sky += float3(1.00, 0.66, 0.36) * glow * 0.75;
    sky = lerp(sky, float3(1.00, 0.96, 0.86), disk);

    float d = horizon - uv.y;
    float z = 1.0 / (d + 0.030);
    float x = (uv.x - 0.5) * z * 0.52;
    float2 wuv = float2(x * 0.55 + t * 0.020, z * 0.72 - t * 0.120);
    float wf = fbm(wuv * 1.8);
    float wave = sin(x * 2.4 + z * 0.8 + t * 0.9) * 0.55
               + sin(x * 5.3 - z * 1.2 - t * 1.3) * 0.25
               + wf * 0.80;
    float crest = smoothstep(0.55, 0.95, wave);
    float3 sea = lerp(float3(0.080, 0.220, 0.260), float3(0.020, 0.070, 0.110), saturate(d * 2.2));
    sea += float3(0.22, 0.32, 0.35) * crest * exp(-d * 3.5) * 0.45;

    float sunpath = exp(-abs(uv.x - sun.x) * (9.0 + z * 0.4));
    float sparkle = 0.55 + 0.45 * sin(x * 10.0 + z * 2.0 + t * 2.0);
    sea += float3(1.00, 0.62, 0.30) * sunpath * sparkle * (0.25 + 0.60 * crest) * smoothstep(0.0, 0.12, d);
    sea = lerp(sea, float3(0.95, 0.58, 0.32), (1.0 - smoothstep(0.0, 0.035, d)) * 0.35);

    return uv.y >= horizon ? sky : sea;
}

float3 aurora_col(float2 uv, float t)
{
    float3 sky = lerp(float3(0.010, 0.018, 0.045), float3(0.020, 0.055, 0.085),
                      smoothstep(0.0, 0.9, uv.y));

    float2 cell = floor(uv * float2(220.0, 120.0));
    float star_hash = hash21(cell);
    float star = smoothstep(0.994, 1.0, star_hash) * (0.5 + 0.5 * sin(t * 2.0 + star_hash * 20.0));
    sky += float3(0.8, 0.88, 1.0) * star * smoothstep(0.2, 0.6, uv.y);

    float cx = uv.x + 0.06 * sin(uv.y * 5.0 + t * 0.12);
    float n = fbm(float2(cx * 2.6 + t * 0.02, uv.y * 0.9 - t * 0.025));
    float band = smoothstep(0.46, 0.68, n)
               * smoothstep(0.05, 0.25, uv.y)
               * (1.0 - smoothstep(0.85, 1.0, uv.y));
    float3 curtain = lerp(float3(0.04, 0.60, 0.34), float3(0.32, 0.95, 0.55), n);
    sky += curtain * band * 0.85;

    float low = smoothstep(0.35, 0.45, uv.y);
    sky = lerp(float3(0.008, 0.012, 0.020), sky, low);
    return sky;
}

float4 PSMain(VSOut i) : SV_Target
{
    // `i.pos` is the pixel; the field math is written for y growing upwards.
    float2 uv = i.pos.xy / g_resolution;
    float2 p = float2(uv.x, 1.0 - uv.y);
    float3 color = aurora_col(p, g_time);
    if (g_mode > 0.5)
    {
        color = ocean_col(p, g_time);
    }
    // A scrim toward the left and the top, where the white text sits.
    float left = 0.74 + 0.20 * smoothstep(0.0, 0.55, p.x);
    float top = 0.88 + 0.12 * smoothstep(0.0, 0.28, 1.0 - p.y);
    return float4(color * (left * top), 1.0);
}
