**No site's frame changes because of the route.** Every pair that could be compared is either pixel-identical or differs by what the site itself served:

- Pixel-identical on all four pairs: youtube, wikipedia, lyft, reddit, x, bing, walmart, microsoft, shopify, weather. facebook (three captures) and apple (three captures) are identical on the pairs captured.
- **google:** one develop capture and one fix capture are pixel-identical (A1 = B2); the other two are different pages of its own (4.09% within develop). Its own variance.
- **linkedin:** 2.96 to 3.76% across. The two arms got different copy from the server: the develop captures say "Discover new opportunities" with "Sign in / Join now" in the header, the fix captures "Welcome to your professional community" with "Join now / Sign in" (`scratch/s1002d/linkedin-ab.png`). The hero illustration is the same in both. Against the stored Chrome frames: develop 19.87%, fix 19.75% and 20.20%.
- **netflix:** 12.9% across on the two pairs captured, 4.89% within the fix arm. Different headline copy again ("Endless entertainment starts at $8.99/mo" against "Unlimited movies, TV shows, and more", and a white against a red button: `scratch/s1002d/netflix-ab.png`). Against Chrome: develop 65.2%, fix 66.1 to 66.2%.
- **yahoo:** one fix capture is a different page (24.58% from the other three, which are identical to each other across arms).
- Not captured inside the binary's 30 s on either arm: github, squarespace (four of four), instagram (three of four), cnn (three of four). The machine was at load 12 to 14; these are the heaviest pages on the board.

So the Referer was not what kept any first-viewport image off these 20 pages today. That is a measured null, and it is the expected one for most CDNs; the change is for the shield and for the background-image fetch that goes on top of it. No scoring board was run.
