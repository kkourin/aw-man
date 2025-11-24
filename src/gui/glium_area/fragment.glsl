#version 140

// Reads will be linear with unassociated alpha
uniform sampler2D tex;
// Linear RGB with premultiplied alpha
uniform highp vec4 bg;
uniform bool grey_alpha;
uniform bool force_apple_p3;

in highp vec2 v_tex_coords;
out highp vec4 f_color;

highp float linear_to_srgb(highp float value) {
    return value <= 0.0031308 ? value * 12.92 : 1.055 * pow(value, 1.0f/2.4f) - 0.055;
}


/*
# rel 100
srgb = np.array(
[[41.24,  21.26,   1.93], # R
[35.76,  71.52,  11.92], # G
[18.05,   7.22,  95.05],# B
]).transpose()

# rel 100
p3 = np.array(
[[ 48.66,  22.90,  -0.00,], # R
 [ 	26.57,  69.17,   4.51, ], # G
 [  19.82,   7.93, 104.39,], # B
 ]).transpose()

print(np.linalg.inv(p3) @ srgb @ [[1],[1],[1]])
np.set_printoptions(precision=7, suppress=True)

[[ 0.8224902  0.1774221  0.0000878]
 [ 0.0331026  0.9669337 -0.0000363]
 [ 0.0170582  0.0724124  0.9105294]]
*/

const highp mat3 p3_to_srgb = mat3(
    0.8224902, 0.0331026, 0.0170582,
    0.1774221, 0.9669337, 0.0724124,
    0.0000878, -0.0000363, 0.9105294
);

void main() {
    highp vec4 src = texture(tex, v_tex_coords);

    highp float src_a = src.a;
    if (grey_alpha) {
      // Swizzling is applied after sRGB->linear conversion, which is very backwards.
      // For grey_alpha images this is necessary to get the real alpha value back.
      src_a = linear_to_srgb(src_a);
    }

    highp float a = (src_a + bg.a * (1.0 - src_a));
    // dst is in linear RGB with premultiplied alpha
    highp vec4 dst =
      vec4((src.rgb * src_a + bg.rgb * (1.0 - src_a)), a);

    if (force_apple_p3) {
        // Convert linear P3 to linear sRGB
        dst.rgb = p3_to_srgb * dst.rgb;
    }

    // f_color is a linear texture incorrectly treated as srgb by GTK.
    // Explicitly write srgb float values into the linear texture.
    //
    // Works for apple P3 too since same tone curve
    dst.r = linear_to_srgb(dst.r);
    dst.g = linear_to_srgb(dst.g);
    dst.b = linear_to_srgb(dst.b);

    f_color = dst;
}

