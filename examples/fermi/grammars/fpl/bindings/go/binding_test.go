package tree_sitter_fpl_test

import (
	"testing"

	tree_sitter "github.com/smacker/go-tree-sitter"
	"github.com/tree-sitter/tree-sitter-fpl"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_fpl.Language())
	if language == nil {
		t.Errorf("Error loading Fpl grammar")
	}
}
