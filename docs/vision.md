The idea is for Photon to be a tool/pipeline that takes images/photos as input and uses various AI models to analyze and extract information from them, ultimately tagging them cleverly (with metadata and possibly other data) for ultra-easy, fast, accurate and sophisticated organization and retrieval.

Example use cases:
- Organizing a large collection of photos
- Extracting information from photos for a database
- Creating a searchable archive of photos

Specific use case example: market team has thousands of photos across multiple folders in Dropbox or Google Drive. A lot of time is lost manually searching and finding suitable photos for campaigns. Photon would automatically tag and organize these photos, making them easily searchable and retrievable. For example, it could identify products in photos and tag them with product names, prices, and other relevant information. Ultimately, a user should be able to type in e.g. "fashion" and then a photo of red sneakers should show up. But those sneakers would also show up if the user searched for "red" or "sneakers" or "footwear".

The goal is to make Photon a maximally powerful tool, so I'm thinking of building it in Rust or Go for ultimate performance.

Database-wise, I'm open to considering all options, but I'm leaning towards PostgreSQL for its robustness and scalability, coupled with pgVector for vector-based retrieval - though I'm open to other options as well, especially if they offer better performance or features.

Help me think through this and come up with a comprehensive plan/blueprint for building Photon.