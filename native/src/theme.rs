use bevy::prelude::*;
use bevy::{
    asset::RenderAssetUsages,
    image::{CompressedImageFormats, ImageSampler, ImageType},
};

pub const INK: Color = Color::srgb(0.035, 0.055, 0.065);
pub const PAPER: Color = Color::srgb(0.91, 0.91, 0.85);
pub const MUTED: Color = Color::srgb(0.57, 0.65, 0.65);
pub const ACCENT: Color = Color::srgb(0.84, 0.68, 0.40);

#[derive(Resource)]
pub struct Fonts {
    pub body: Handle<Font>,
    pub display: Handle<Font>,
}
#[derive(Component)]
pub struct Panel;
#[derive(Resource)]
pub struct FieldArt {
    button: Handle<Image>,
    panel: Handle<Image>,
}
fn texture(bytes: &[u8]) -> Image {
    Image::from_buffer(
        bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::linear(),
        RenderAssetUsages::default(),
    )
    .expect("embedded field UI art")
}
fn sliced(image: Handle<Image>, border: f32, scale: f32) -> ImageNode {
    ImageNode {
        image,
        image_mode: NodeImageMode::Sliced(TextureSlicer {
            border: BorderRect::all(border),
            max_corner_scale: scale,
            center_scale_mode: SliceScaleMode::Stretch,
            sides_scale_mode: SliceScaleMode::Stretch,
        }),
        ..default()
    }
}
pub fn setup(
    mut commands: Commands,
    mut fonts: ResMut<Assets<Font>>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.insert_resource(FieldArt {
        button: images.add(texture(include_bytes!("../assets/ui/field-button.png"))),
        panel: images.add(texture(include_bytes!("../assets/ui/field-panel.png"))),
    });
    commands.insert_resource(Fonts {
        body: fonts.add(
            Font::try_from_bytes(include_bytes!("../assets/fonts/Barlow-Regular.ttf").to_vec())
                .expect("embedded body font"),
        ),
        display: fonts.add(
            Font::try_from_bytes(
                include_bytes!("../assets/fonts/BarlowCondensed-SemiBold.ttf").to_vec(),
            )
            .expect("embedded title font"),
        ),
    });
}
/// Native labels remain crisp; the image supplies physical edges and surface detail.
pub fn skin(
    mut commands: Commands,
    art: Res<FieldArt>,
    buttons: Query<Entity, (Added<Button>, Without<ImageNode>)>,
    panels: Query<Entity, (Added<Panel>, Without<ImageNode>)>,
    mut painted: Query<(&Interaction, Option<&BackgroundColor>, &mut ImageNode), With<Button>>,
) {
    let _timing = crate::performance::span("theme.skin");
    for entity in &buttons {
        commands
            .entity(entity)
            .insert(sliced(art.button.clone(), 88., 0.12));
    }
    for entity in &panels {
        commands
            .entity(entity)
            .insert(sliced(art.panel.clone(), 65., 0.25));
    }
    for (interaction, background, mut image) in &mut painted {
        let selected = background.is_some_and(|c| {
            let c = c.0.to_srgba();
            c.blue > c.red * 1.3 && c.blue > 0.22
        });
        let color = match interaction {
            Interaction::Pressed => Color::srgb(0.70, 0.77, 0.65),
            Interaction::Hovered => Color::srgb(1.18, 1.13, 0.91),
            Interaction::None if selected => Color::srgb(0.78, 1.10, 1.18),
            Interaction::None => Color::srgb(0.95, 0.97, 0.88),
        };
        if image.color != color {
            image.color = color;
        }
    }
}
pub fn style_text(fonts: Res<Fonts>, mut text: Query<&mut TextFont, Added<TextFont>>) {
    for mut font in &mut text {
        if font.font == Handle::default() {
            font.font = if font.font_size >= 22. {
                fonts.display.clone()
            } else {
                fonts.body.clone()
            };
        }
    }
}
