document
  .querySelectorAll("pre code.language-bash, pre code.language-diff")
  .forEach((block) => hljs.highlightElement(block));
