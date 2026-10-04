# Masks

A mask shows one subtree only as far as another covers it. The source is the shape and is never shown
itself; the content is what is seen through it: by the source's alpha where the two overlap, and not at all
where the source drew nothing. A word set over a field is a window onto the field.

```rsx
mask width:100% height:60sh label:"SIMULACIÓN"
    text "SIMULACIÓN" font_size:18sw font_weight:900 text_align:center   // the source
    canvas paint:field width:100% height:100%                            // the content
```

```rust
Mask::new(layout_style, box_item(source), box_item(content))?
```

The box is laid out by its own style with the content in its flow. The source is laid over the whole box,
centred down its height, so a word that fills the width is a window the height of its line. Only the content
answers the pointer. A `mask` holds exactly two children; anything else is a build error.

## The pair of layers

Every target draws a mask from the same two layers, `DrawCommand::PushLayer` with `mask: LayerMask::Source` and
then `mask: LayerMask::Apply` (`RenderNode::masked` builds them). The source is drawn and kept aside; the layer
right after it is composited through it and the source is let go. A source that drew nothing hides its content
entirely. Nested masks work: each applied layer keeps the source it was opened with.

| Target | A mask is |
| --- | --- |
| GPU | Two layer textures: the content composites through a shader that multiplies it by the source's alpha, sampled where each pixel lands. A blend mode that blends in shader reads its backdrop from the group the mask takes, so a masked layer uses its own blend when fixed-function blending computes it, and `Normal` otherwise. Neither layer is cached between frames. |
| Software | The source's pixmap becomes a `tiny_skia::Mask` over the content's, applied before the content composites. |
| Web, document | The box is a drawing, so the pair is an SVG: the source as a `<mask>` with `mask-type: alpha`, the content as a group shown through it. Boxes inside it are placed where layout put them, each with its own frame, `clip`, `rotate` and `scale` about its own `transform_origin`, as the same box laid out as an element has them. A text inside the source is an SVG `<text>`, one line, on the baseline the face's metrics give, because a browser does not draw a `foreignObject` inside a mask. Everything in a drawing is a picture rather than elements of the page, so name the mask with `label:`. |
| Web, canvas | As the GPU. |
| Terminal | The content drawn whole and the source not at all: a cell has no alpha to show anything through. |

## Limits

- In a document a mask is a picture: its content is not selectable text, its boxes are not focusable, and a
  reader hears the `label:` the mask was given.
- A text in a mask source is one line on the web, so a source meant to wrap belongs in two texts.
