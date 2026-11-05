sampler2D input : register(s0);
float time : register(c0);
float2 resolution : register(c1);

float hash21(float2 p)
{
    return frac(sin(dot(p, float2(127.1, 311.7))) * 43758.5453);
}

float noise(float2 p)
{
    float2 i = floor(p);
    float2 f = frac(p);
    f = f * f * (3.0 - 2.0 * f);
    float a = hash21(i);
    float b = hash21(i + float2(1.0, 0.0));
    float c = hash21(i + float2(0.0, 1.0));
    float d = hash21(i + float2(1.0, 1.0));
    return lerp(lerp(a, b, f.x), lerp(c, d, f.x), f.y);
}

float fbm(float2 p)
{
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 4; i++)
    {
        v += a * noise(p);
        p = p * 2.03 + float2(1.7, 9.2);
        a *= 0.5;
    }
    return v;
}

float4 main(float2 tex : TEXCOORD0) : COLOR0
{
    float2 uv = float2(tex.x, 1.0 - tex.y);
    float horizon = 0.52;
    float sunX = 0.63;
    float sunY = 0.555;
    float aspect = resolution.x / resolution.y;

    float3 col = float3(0.0, 0.0, 0.0);

    // Sky
    float sy = saturate(uv.y / horizon);
    float3 skyLow = float3(1.00, 0.55, 0.26);
    float3 skyMid = float3(0.55, 0.46, 0.48);
    float3 skyTop = float3(0.10, 0.16, 0.30);
    float3 sky = lerp(skyLow, skyMid, smoothstep(0.0, 0.55, sy));
    sky = lerp(sky, skyTop, smoothstep(0.45, 1.0, sy));

    float cl1 = fbm(float2(uv.x * 2.1 + time * 0.012, uv.y * 4.2 + 1.3));
    float cl2 = fbm(float2(uv.x * 4.6 - time * 0.018, uv.y * 7.0 + 3.7));
    float cloud = smoothstep(0.44, 0.66, cl1 * 0.62 + cl2 * 0.48);
    cloud *= smoothstep(horizon + 0.01, horizon + 0.20, uv.y);
    float3 cloudCol = lerp(float3(0.16, 0.17, 0.24), float3(1.00, 0.68, 0.44),
        saturate((uv.y - horizon) * 1.6 + cl1 * 0.35));
    sky = lerp(sky, cloudCol, cloud * 0.88);

    float2 sp = float2((uv.x - sunX) * aspect, uv.y - sunY);
    float sd = length(sp);
    float disk = smoothstep(0.055, 0.040, sd);
    float glow = exp(-sd * 3.6);
    sky += float3(1.00, 0.70, 0.40) * glow * 0.70;
    sky = lerp(sky, float3(1.00, 0.95, 0.82), disk);

    // Ocean
    float d = horizon - uv.y;
    float z = 1.0 / (d + 0.035);
    float px = (uv.x - 0.5) * z * 0.55;
    float wz = z * 0.85 - time * 0.35;
    float waveA = sin(px * 2.2 + wz * 0.6 + time * 0.9);
    float waveB = sin(px * 4.7 - wz * 1.1 - time * 1.4);
    float wf = fbm(float2(px * 0.55 + time * 0.03, wz * 0.8));
    float wave = waveA * 0.45 + waveB * 0.25 + wf * 0.70;
    float crest = smoothstep(0.55, 0.98, wave);

    float3 deep = float3(0.015, 0.060, 0.105);
    float3 mid = float3(0.055, 0.170, 0.230);
    float3 ocean = lerp(mid, deep, saturate(d * 2.2));
    ocean += float3(0.25, 0.42, 0.45) * crest * exp(-d * 3.0) * 0.55;

    float path = exp(-abs(uv.x - sunX) * (9.0 + z * 0.5));
    float sparkle = 0.55 + 0.45 * sin(px * 9.0 + wz * 2.0 + time * 2.0);
    ocean += float3(1.00, 0.62, 0.30) * path * sparkle * (0.30 + 0.65 * crest) * smoothstep(0.0, 0.12, d);

    ocean = lerp(ocean, float3(0.95, 0.55, 0.30), smoothstep(0.035, 0.0, d) * 0.35);

    col = uv.y < horizon ? ocean : sky;
    float hline = exp(-abs(uv.y - horizon) * 120.0);
    col += float3(1.00, 0.62, 0.32) * hline * 0.22;

    float2 q = uv - 0.5;
    col *= 1.0 - dot(q, q) * 0.35;

    return float4(col, 1.0);
}
