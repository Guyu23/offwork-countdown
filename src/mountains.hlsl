// Piau-Engaly by athibaul
// https://www.shadertoy.com/view/stGcRV
// A sunrise over the French Pyrénées. Hash helpers by Dave Hoskins
// (https://www.shadertoy.com/view/4djSRW). The sound pass is not used.
//
// GLSL mat2(a, b, c, d) is column-major. The float2x2 below is that matrix,
// and mul(v, m) is GLSL's v * m.

cbuffer Globals : register(b0)
{
    float3 iResolution;
    float iTime;
};

float hash12(float2 p)
{
    float3 p3 = frac(float3(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return frac((p3.x + p3.y) * p3.z);
}

float noise(float2 p)
{
    float2 ip = floor(p);
    float2 fp = frac(p);
    fp = smoothstep(0.0, 1.0, fp);
    return lerp(
        lerp(hash12(ip + float2(0, 0)), hash12(ip + float2(1, 0)), fp.x),
        lerp(hash12(ip + float2(0, 1)), hash12(ip + float2(1, 1)), fp.x),
        fp.y);
}

float fbm(float2 p, int lv)
{
    float a = 1.0;
    float t = 0.0;
    [loop]
    for (int i = 0; i < 8; i++)
    {
        if (i >= lv)
        {
            break;
        }
        p += float2(13.102, 1.535);
        t += a * noise(p);
        p = mul(p, float2x2(3.0, -4.0, 4.0, 3.0) * 0.4);
        a *= 0.5;
    }
    return 0.5 * t;
}

struct VSOut
{
    float4 position : SV_Position;
};

float4 PSMain(VSOut input) : SV_Target
{
    float2 fragCoord = float2(input.position.x, iResolution.y - input.position.y);
    float2 uv = (2.0 * fragCoord - iResolution.xy) / iResolution.y;

    float mtHeight = fbm(uv.xx + 0.6, 8);
    float mtHeightSm = fbm(uv.xx + 0.6, 3);
    float3 col = float3(0.0, 0.0, 0.0);
    float2 sunPos = float2(0.8, -0.8);
    float3 skyCol = float3(0.075, 0.310, 0.518);
    float q = uv.y - sunPos.y;
    float q2 = uv.x - sunPos.x;
    skyCol = lerp(skyCol, float3(0.482, 0.580, 0.902), exp(-0.5 * q * q - 0.2 * q2 * q2));
    float3 cloudCol = lerp(skyCol, float3(0.5, 0.5, 0.5), 0.2);
    skyCol = lerp(skyCol, float3(0.706, 0.851, 0.953), exp(-q * q * 3.0 - 0.5 * q2 * q2));
    skyCol = lerp(skyCol, float3(0.980, 0.5, 0.3), exp(-q * q * 10.0 - 0.5 * q2 * q2));
    skyCol = lerp(skyCol, float3(1.0, 1.0, 0.7), exp(-3.0 * length(uv - sunPos)));
    float3 cloudCol3 = lerp(float3(0.980, 0.5, 0.3), float3(1.0, 1.0, 0.7), exp(-length(uv - sunPos)));
    cloudCol3 = lerp(cloudCol3, cloudCol, smoothstep(0.0, -1.0, uv.y + 0.3 * uv.x));

    float w = 1.5 * length(fwidth(uv));
    float ridge = uv.y + 0.3 * uv.x + 0.2 * max(uv.x, 0.0) + 1.0;
    float isSky = smoothstep(0.0, w, ridge - mtHeight);
    col = lerp(col, skyCol, isSky);
    col = lerp(col, skyCol, 0.5 * smoothstep(-0.5, 0.1, ridge - mtHeightSm));

    float2 fuv = frac(0.1 * uv);
    float2 uvv = 20.0 * fuv * (1.0 - fuv) * (0.5 - fuv);
    uvv = float2(1.0, -1.0) * uvv.yx;
    float2 uv2 = uv + uvv * cos(0.1 * iTime);

    float silver = fbm(30.0 * uv - 0.06 * iTime, 8) + 30.0 * (uv.y + 0.8);
    silver = smoothstep(0.0, 1.0, silver) * smoothstep(2.0, 1.0, silver) * 1.0 / (1.0 + 500.0 * q2 * q2) * isSky;
    col += silver * float3(0.9, 0.6, 0.3) * 100.0;

    float lowClouds = fbm(5.0 * uv + 0.1 * iTime, 8);
    float midClouds = fbm(3.0 * uv + float2(0.06, -0.03) * iTime, 8);
    float hiClouds = fbm(uv2 + float2(0.1, 0.01) * iTime, 8) - 0.5;
    float hiClouds2 = fbm(uv + 10.0 + float2(0.062, -0.03) * iTime, 8) - 0.5;
    col = lerp(col, cloudCol3, 0.5 * smoothstep(0.0, 1.0, -uv.y + 3.0 * hiClouds));
    col = lerp(col, cloudCol3, 0.5 * smoothstep(0.0, 1.0, -uv.y + 3.0 * hiClouds2));
    col = lerp(col, float3(0.9, 0.6, 0.3) * 100.0, 1.0 / (1.0 + 2000.0 * (q * q + q2 * q2)));
    col += float3(0.9, 0.6, 0.3) * 2.0 / (1.0 + 10.0 * sqrt(q * q + 0.3 * q2 * q2));
    col = lerp(col, 0.8 * cloudCol, 0.8 * smoothstep(0.0, 1.0, -2.0 * (uv.y + 0.5) + midClouds));
    col = lerp(col, 0.5 * cloudCol, smoothstep(0.0, 1.0, -3.0 * (uv.y + 0.8) + lowClouds));

    float luma = max(col.r + col.g + col.b, 1e-4);
    col = lerp(col, col * pow(col / luma, (float3)(dot(uv, uv) * 0.3)), 0.2);
    col = pow(max(col, 0.0), (float3)(2.2));
    col = (col * (2.51 * col + 0.03)) / (col * (2.43 * col + 0.59) + 0.14);
    col = pow(max(col, 0.0), (float3)(1.0 / 2.2));

    col += 0.03 * (hash12(fragCoord) - 0.5) * sqrt(iResolution.y / 400.0);
    return float4(col, 1.0);
}
