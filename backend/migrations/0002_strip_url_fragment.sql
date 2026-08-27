-- Product URLs scraped before 0.1.1 carried the tile's "#reviews" fragment; drop it.
UPDATE offers SET product_url = split_part(product_url, '#', 1) WHERE product_url LIKE '%#%';
