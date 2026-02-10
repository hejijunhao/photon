#!/usr/bin/env python3
"""Generate WordNet nouns vocabulary for Photon.

Output format: term<TAB>synset_id<TAB>hypernym_chain (pipe-separated)
Example: labrador_retriever	02099712	retriever|sporting_dog|dog|canine|carnivore|mammal|animal|organism|entity

Uses first lemma per synset only (~82K terms, ~6MB).
"""

import sys
from nltk.corpus import wordnet as wn


def get_hypernym_chain(synset):
    """Get the hypernym chain from a synset up to the root."""
    chain = []
    current = synset
    seen = set()
    while True:
        hypernyms = current.hypernyms()
        if not hypernyms:
            break
        parent = hypernyms[0]  # Take first (most common) hypernym
        if parent.offset() in seen:
            break
        seen.add(parent.offset())
        chain.append(parent.lemmas()[0].name())
        current = parent
    return chain


def main():
    output_path = sys.argv[1] if len(sys.argv) > 1 else "data/vocabulary/wordnet_nouns.txt"

    noun_synsets = list(wn.all_synsets(pos=wn.NOUN))
    print(f"Found {len(noun_synsets)} noun synsets in WordNet", file=sys.stderr)

    lines = []
    seen_names = set()

    for synset in noun_synsets:
        # Use only the first lemma as the primary term
        lemma = synset.lemmas()[0]
        name = lemma.name()

        if name in seen_names:
            continue
        seen_names.add(name)

        synset_id = f"{synset.offset():08d}"
        hypernym_chain = get_hypernym_chain(synset)
        chain_str = "|".join(hypernym_chain) if hypernym_chain else ""

        lines.append(f"{name}\t{synset_id}\t{chain_str}")

    lines.sort()

    with open(output_path, "w") as f:
        f.write("# WordNet nouns vocabulary for Photon\n")
        f.write("# Format: term<TAB>synset_id<TAB>hypernym_chain (pipe-separated)\n")
        f.write(f"# Generated from WordNet 3.0 — {len(lines)} terms\n")
        for line in lines:
            f.write(line + "\n")

    print(f"Wrote {len(lines)} terms to {output_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
