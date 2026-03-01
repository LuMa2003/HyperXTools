"""Generate branded WiX installer images from the project logo."""

from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ASSETS_DIR = Path(__file__).resolve().parent.parent / "assets"
LOGO_PATH = ASSETS_DIR / "logo.png"

BG_COLOR = (240, 240, 240)  # light gray, readable with black WiX text
TEXT_COLOR = (30, 30, 30)

# Banner: 493x58, shown on most dialog pages
BANNER_SIZE = (493, 58)
# Dialog: 493x312, left panel on Welcome and Exit dialogs
DIALOG_SIZE = (493, 312)

FONT_NAME = "segoeui.ttf"  # Segoe UI, available on all Windows 10/11


def load_logo():
    logo = Image.open(LOGO_PATH).convert("RGBA")
    return logo


def get_font(size):
    try:
        return ImageFont.truetype(FONT_NAME, size)
    except OSError:
        # Fallback to default font
        return ImageFont.load_default()


def create_banner(logo: Image.Image):
    """Create the 493x58 top banner image.

    WiX places its own page title and description text on the left side
    of the banner, so we only put the logo on the far right.
    """
    img = Image.new("RGB", BANNER_SIZE, BG_COLOR)

    # Scale logo to fit in the right portion, with padding
    padding = 6
    max_logo_h = BANNER_SIZE[1] - 2 * padding
    max_logo_w = 80
    logo_ratio = logo.width / logo.height
    logo_h = max_logo_h
    logo_w = int(logo_h * logo_ratio)
    if logo_w > max_logo_w:
        logo_w = max_logo_w
        logo_h = int(logo_w / logo_ratio)
    logo_resized = logo.resize((logo_w, logo_h), Image.LANCZOS)

    # Place logo on the far right
    logo_x = BANNER_SIZE[0] - logo_w - 12
    logo_y = (BANNER_SIZE[1] - logo_h) // 2
    img.paste(logo_resized, (logo_x, logo_y), logo_resized)

    return img


def create_dialog(logo: Image.Image):
    """Create the 493x312 dialog background image.

    WiX overlays its Welcome/Exit text in the right ~60% of the dialog.
    We place the logo and app name on the left side to avoid overlap.
    """
    img = Image.new("RGB", DIALOG_SIZE, BG_COLOR)
    draw = ImageDraw.Draw(img)

    # The left column for branding is roughly 0..180px
    left_col_center = 100

    # Scale logo to fit in the left column
    max_logo_size = 120
    logo_ratio = logo.width / logo.height
    if logo_ratio > 1:
        logo_w = max_logo_size
        logo_h = int(logo_w / logo_ratio)
    else:
        logo_h = max_logo_size
        logo_w = int(logo_h * logo_ratio)
    logo_resized = logo.resize((logo_w, logo_h), Image.LANCZOS)

    # Center logo in the left column, vertically centered a bit above middle
    logo_x = left_col_center - logo_w // 2
    logo_y = 70
    img.paste(logo_resized, (logo_x, logo_y), logo_resized)

    # Draw "HyperXTools" text below the logo, centered in left column
    font = get_font(18)
    text = "HyperXTools"
    bbox = draw.textbbox((0, 0), text, font=font)
    text_w = bbox[2] - bbox[0]
    text_x = left_col_center - text_w // 2
    text_y = logo_y + logo_h + 12 - bbox[1]
    draw.text((text_x, text_y), text, fill=TEXT_COLOR, font=font)

    return img


def main():
    logo = load_logo()

    banner = create_banner(logo)
    banner_path = ASSETS_DIR / "installer-banner.bmp"
    banner.save(banner_path, "BMP")
    print(f"Created {banner_path} ({BANNER_SIZE[0]}x{BANNER_SIZE[1]})")

    dialog = create_dialog(logo)
    dialog_path = ASSETS_DIR / "installer-dialog.bmp"
    dialog.save(dialog_path, "BMP")
    print(f"Created {dialog_path} ({DIALOG_SIZE[0]}x{DIALOG_SIZE[1]})")


if __name__ == "__main__":
    main()
