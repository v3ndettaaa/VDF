/*
 * C shims around MuPDF calls that may fz_throw.
 *
 * MuPDF reports errors with setjmp/longjmp (fz_try/fz_catch). A longjmp
 * across the FFI boundary into Rust is undefined behaviour, so every
 * fallible entry point from Rust goes through one of these shims: the
 * try/catch is fully contained in C, and the shim returns 0 on success or
 * stores an error message and returns 1 on failure.
 *
 * Every shim takes errbuf/errlen and writes the caught message there
 * (truncated, always NUL-terminated).
 */
#include <stdio.h>
#include <string.h>
#include "mupdf/fitz.h"

static void store_err(fz_context *ctx, char *errbuf, size_t errlen)
{
	const char *msg = fz_caught_message(ctx);
	if (!msg)
		msg = "unknown mupdf error";
	snprintf(errbuf, errlen, "%s", msg);
}

int vdf_open_document(fz_context *ctx, const char *path, fz_document **out, char *errbuf, size_t errlen)
{
	fz_document *doc = NULL;
	fz_try(ctx)
		doc = fz_open_document(ctx, path);
	fz_catch(ctx) {
		store_err(ctx, errbuf, errlen);
		return 1;
	}
	*out = doc;
	return 0;
}

int vdf_open_document_with_stream(fz_context *ctx, const char *magic, fz_stream *stm, fz_document **out, char *errbuf, size_t errlen)
{
	fz_document *doc = NULL;
	fz_try(ctx)
		doc = fz_open_document_with_stream(ctx, magic, stm);
	fz_catch(ctx) {
		store_err(ctx, errbuf, errlen);
		return 1;
	}
	*out = doc;
	return 0;
}

int vdf_load_page(fz_context *ctx, fz_document *doc, int number, fz_page **out, char *errbuf, size_t errlen)
{
	fz_page *page = NULL;
	fz_try(ctx)
		page = fz_load_page(ctx, doc, number);
	fz_catch(ctx) {
		store_err(ctx, errbuf, errlen);
		return 1;
	}
	*out = page;
	return 0;
}

int vdf_bound_page(fz_context *ctx, fz_page *page, fz_rect *out, char *errbuf, size_t errlen)
{
	fz_try(ctx)
		*out = fz_bound_page(ctx, page);
	fz_catch(ctx) {
		store_err(ctx, errbuf, errlen);
		return 1;
	}
	return 0;
}

int vdf_run_page(fz_context *ctx, fz_page *page, fz_device *dev, fz_matrix ctm, char *errbuf, size_t errlen)
{
	fz_try(ctx)
		fz_run_page(ctx, page, dev, ctm, NULL);
	fz_catch(ctx) {
		store_err(ctx, errbuf, errlen);
		return 1;
	}
	return 0;
}

int vdf_new_draw_device(fz_context *ctx, fz_matrix ctm, fz_pixmap *dest, fz_device **out, char *errbuf, size_t errlen)
{
	fz_device *dev = NULL;
	fz_try(ctx)
		dev = fz_new_draw_device(ctx, ctm, dest);
	fz_catch(ctx) {
		store_err(ctx, errbuf, errlen);
		return 1;
	}
	*out = dev;
	return 0;
}

int vdf_new_pixmap(fz_context *ctx, fz_colorspace *cs, fz_irect bbox, int alpha, fz_pixmap **out, char *errbuf, size_t errlen)
{
	fz_pixmap *pix = NULL;
	fz_try(ctx)
		pix = fz_new_pixmap_with_bbox(ctx, cs, bbox, NULL, alpha);
	fz_catch(ctx) {
		store_err(ctx, errbuf, errlen);
		return 1;
	}
	*out = pix;
	return 0;
}

int vdf_load_outline(fz_context *ctx, fz_document *doc, fz_outline **out, char *errbuf, size_t errlen)
{
	fz_outline *outline = NULL;
	fz_try(ctx)
		outline = fz_load_outline(ctx, doc);
	fz_catch(ctx) {
		store_err(ctx, errbuf, errlen);
		return 1;
	}
	*out = outline;
	return 0;
}

int vdf_count_pages(fz_context *ctx, fz_document *doc, int *out, char *errbuf, size_t errlen)
{
	fz_try(ctx)
		*out = fz_count_pages(ctx, doc);
	fz_catch(ctx) {
		store_err(ctx, errbuf, errlen);
		return 1;
	}
	return 0;
}

int vdf_authenticate_password(fz_context *ctx, fz_document *doc, const char *pw, int *out, char *errbuf, size_t errlen)
{
	fz_try(ctx)
		*out = fz_authenticate_password(ctx, doc, pw);
	fz_catch(ctx) {
		store_err(ctx, errbuf, errlen);
		return 1;
	}
	return 0;
}

/*
 * Render one tile from a display list: pixmap (clipped to its own bbox) +
 * draw device (identity) + run display list. Display lists are the
 * MuPDF-supported unit of concurrent rendering (docs/reference/c/overview.md
 * §Multi-threading rule 2): the document must NOT be touched from several
 * threads at once, but a finished display list may be run from many.
 * All object lifetime is contained here; the caller drops the pixmap after
 * copying samples.
 */
int vdf_render_list_tile(
	fz_context *ctx,
	fz_display_list *list,
	fz_colorspace *cs,
	fz_matrix ctm,
	fz_irect bbox,
	int alpha,
	fz_pixmap **out,
	char *errbuf,
	size_t errlen)
{
	fz_pixmap *pix = NULL;
	fz_device *dev = NULL;
	int rc = 1;

	fz_var(pix);
	fz_var(dev);

	fz_try(ctx)
	{
		pix = fz_new_pixmap_with_bbox(ctx, cs, bbox, NULL, alpha);
		fz_clear_pixmap_with_value(ctx, pix, 0xff);
		dev = fz_new_draw_device(ctx, fz_identity, pix);
		fz_run_display_list(ctx, list, dev, ctm,
			fz_make_rect(0, 0, (float)bbox.x1, (float)bbox.y1), NULL);
		fz_drop_device(ctx, dev);
		dev = NULL;
		*out = pix;
		rc = 0;
	}
	fz_catch(ctx)
	{
		store_err(ctx, errbuf, errlen);
		if (dev)
			fz_drop_device(ctx, dev);
		if (pix)
			fz_drop_pixmap(ctx, pix);
		*out = NULL;
		rc = 1;
	}
	return rc;
}

int vdf_new_display_list_from_page(
	fz_context *ctx,
	fz_page *page,
	fz_display_list **out,
	char *errbuf,
	size_t errlen)
{
	fz_display_list *list = NULL;
	fz_try(ctx)
		list = fz_new_display_list_from_page(ctx, page);
	fz_catch(ctx) {
		store_err(ctx, errbuf, errlen);
		return 1;
	}
	*out = list;
	return 0;
}

/*
 * Render one tile: pixmap (clipped to its own bbox) + draw device + run page.
 * Single-threaded path only (rule 2 forbids concurrent document access).
 * All MuPDF object lifetime is contained here; on success the caller drops
 * the returned pixmap after copying samples.
 */
int vdf_render_tile(
	fz_context *ctx,
	fz_page *page,
	fz_colorspace *cs,
	fz_matrix ctm,
	fz_irect bbox,
	int alpha,
	fz_pixmap **out,
	char *errbuf,
	size_t errlen)
{
	fz_pixmap *pix = NULL;
	fz_device *dev = NULL;
	int rc = 1;

	fz_var(pix);
	fz_var(dev);

	fz_try(ctx)
	{
		pix = fz_new_pixmap_with_bbox(ctx, cs, bbox, NULL, alpha);
		fz_clear_pixmap_with_value(ctx, pix, 0xff);
		/* The draw device maps page-content space into the pixmap with
		 * identity; the page→device transform is supplied by fz_run_page.
		 * Passing ctm to both would apply it twice. */
		dev = fz_new_draw_device(ctx, fz_identity, pix);
		fz_run_page(ctx, page, dev, ctm, NULL);		fz_drop_device(ctx, dev);
		dev = NULL;
		*out = pix;
		rc = 0;
	}
	fz_catch(ctx)
	{
		store_err(ctx, errbuf, errlen);
		if (dev)
			fz_drop_device(ctx, dev);
		if (pix)
			fz_drop_pixmap(ctx, pix);
		*out = NULL;
		rc = 1;
	}
	return rc;
}

/* Runs `cb(ctx, user)` inside fz_try so longjmp stays in C. */
int vdf_call_protected(
	fz_context *ctx,
	void (*cb)(fz_context *ctx, void *user),
	void *user,
	char *errbuf,
	size_t errlen)
{
	fz_try(ctx)
		cb(ctx, user);
	fz_catch(ctx)
	{
		store_err(ctx, errbuf, errlen);
		return 1;
	}
	return 0;
}
