"""Stage 1 of a two-commit split: save the full renderer lib.rs aside, and write HEAD's lib.rs plus only the
image-pipeline edits. usage: split_stage1.py   (then check, commit pipeline.rs + lib.rs, run split_stage2.py)"""
import shutil, subprocess
WT = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics'
P = WT + '/crates/rustkit-renderer/src/lib.rs'
FULL = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1001d/renderer-lib-full.rs'
shutil.copy2(P, FULL)
full = open(FULL, newline='').read()
head = subprocess.run(['git', 'show', 'HEAD:crates/rustkit-renderer/src/lib.rs'], cwd=WT, capture_output=True).stdout.decode()
EDITS = [
    ('    color_glyph_pipeline: wgpu::RenderPipeline,\n',
     '    color_glyph_pipeline: wgpu::RenderPipeline,\n'
     '    // Image pipeline: the blit shader composited source-over (straight alpha)\n'
     '    image_pipeline: wgpu::RenderPipeline,\n'),
    ('        // Create blit pipeline for Rgba8Unorm targets (blitting to filter textures)\n',
     '        // Image pipeline: blit shader + source-over blend for straight alpha.\n'
     '        let image_pipeline = pipeline::create_image_pipeline(\n'
     '            &device,\n'
     '            surface_format,\n'
     '            &uniform_bind_group_layout,\n'
     '            &texture_bind_group_layout,\n'
     '        );\n\n'
     '        // Create blit pipeline for Rgba8Unorm targets (blitting to filter textures)\n'),
    ('            color_glyph_pipeline,\n            blit_pipeline_rgba,\n',
     '            color_glyph_pipeline,\n            image_pipeline,\n            blit_pipeline_rgba,\n'),
    ('        // blit_pipeline, not texture_pipeline: the texture shader treats the\n'
     '        // sampled R channel as glyph-atlas alpha; blit samples real RGBA.\n'
     '        render_pass.set_pipeline(&self.blit_pipeline);\n',
     '        // image_pipeline, not texture_pipeline: the texture shader treats the\n'
     '        // sampled R channel as glyph-atlas alpha; the blit shader samples real\n'
     '        // RGBA. Not blit_pipeline either: its blend is REPLACE, which paints an\n'
     "        // image's transparent texels as their own colour (black, for most PNGs).\n"
     '        render_pass.set_pipeline(&self.image_pipeline);\n'),
    # The blend pin, placed where the full file has it.
    ('    #[test]\n    fn no_rounded_constraint_passes_the_quad_through_untouched() {',
     '    /// Images are composited source-over. With the blit pipeline\'s REPLACE a\n'
     '    /// transparent PNG painted black where it should show the page.\n'
     '    #[test]\n'
     '    fn images_are_blended_not_copied() {\n'
     '        assert_eq!(pipeline::IMAGE_BLEND, wgpu::BlendState::ALPHA_BLENDING);\n'
     '        assert_ne!(pipeline::IMAGE_BLEND, wgpu::BlendState::REPLACE);\n'
     '    }\n\n'
     '    #[test]\n    fn no_rounded_constraint_passes_the_quad_through_untouched() {'),
]
s = head
for old, new in EDITS:
    assert s.count(old) == 1, old[:70]
    assert new in full, new[:70]
    s = s.replace(old, new)
open(P, 'w', newline='').write(s)
print('stage 1 written;', len(s.splitlines()), 'lines; full saved', len(full.splitlines()), 'lines')
