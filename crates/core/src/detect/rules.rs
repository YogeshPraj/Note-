//! The tokens each programming language is known for, with their weights.

use Token::{Ends, Has, Starts};

/// What a token looks for in a line of the sample.
#[derive(Clone, Copy)]
pub enum Token {
    /// The text appears anywhere.
    Has(&'static str),
    /// Some line starts with it (after indentation).
    Starts(&'static str),
    /// Some line ends with it.
    Ends(&'static str),
}

/// A language and the tokens that point to it, each with its weight.
pub struct Rule {
    pub label: &'static str,
    pub tokens: &'static [(Token, u32)],
}

pub const RULES: &[Rule] = &[
    Rule {
        label: "Python",
        tokens: &[
            (Starts("def "), 2),
            (Starts("import "), 1),
            (Starts("from "), 1),
            (Has(" import "), 1),
            (Has("self."), 2),
            (Starts("elif "), 2),
            (Has("__name__"), 3),
            (Starts("class "), 1),
            (Has("print("), 1),
            (Ends("):"), 2),
            (Starts("async def "), 2),
        ],
    },
    Rule {
        label: "Rust",
        tokens: &[
            (Starts("fn "), 2),
            (Starts("pub fn "), 3),
            (Has("let mut "), 3),
            (Starts("impl "), 3),
            (Starts("use std::"), 3),
            (Has("println!"), 3),
            (Has("&self"), 3),
            (Has(" -> "), 1),
            (Starts("#["), 2),
            (Has("::new("), 1),
        ],
    },
    Rule {
        label: "Go",
        tokens: &[
            (Starts("package "), 2),
            (Starts("func "), 3),
            (Has(" := "), 2),
            (Has("fmt."), 3),
            (Starts("import ("), 3),
            (Has("go func"), 2),
            (Has("err != nil"), 3),
        ],
    },
    Rule {
        label: "C#",
        tokens: &[
            (Starts("using System"), 4),
            (Starts("namespace "), 2),
            (Has("Console.Write"), 4),
            (Has("{ get; set; }"), 4),
            (Has("public class "), 1),
            (Has("static void Main"), 4),
            (Has("string[]"), 1),
            (Has("var "), 1),
            (Has(" async Task"), 3),
        ],
    },
    Rule {
        label: "Java",
        tokens: &[
            (Has("public static void main"), 4),
            (Has("System.out.print"), 4),
            (Starts("import java."), 4),
            (Starts("package "), 1),
            (Has("public class "), 1),
            (Has("@Override"), 3),
            (Has("private final "), 2),
            (Has("new ArrayList<"), 3),
        ],
    },
    Rule {
        label: "TypeScript",
        tokens: &[
            (Starts("interface "), 3),
            (Has(": string"), 2),
            (Has(": number"), 2),
            (Has(": boolean"), 2),
            (Starts("export "), 1),
            (Starts("type "), 1),
            (Has(" as const"), 3),
            (Has("<T>"), 2),
            (Has("import type "), 3),
            (Has(": Promise<"), 3),
        ],
    },
    Rule {
        label: "JavaScript",
        tokens: &[
            (Has("function "), 1),
            (Starts("const "), 2),
            (Starts("let "), 1),
            (Has("=>"), 2),
            (Has("console.log"), 3),
            (Has("require("), 2),
            (Has("module.exports"), 3),
            (Has("document."), 2),
            (Has(" === "), 2),
            (Starts("export default"), 2),
            (Starts("import "), 1),
        ],
    },
    Rule {
        label: "C++",
        tokens: &[
            (Starts("#include <"), 2),
            (Has("std::"), 3),
            (Has("cout <<"), 4),
            (Starts("template <"), 3),
            (Starts("template<"), 3),
            (Starts("namespace "), 1),
            (Has("nullptr"), 2),
            (Has("#include <iostream>"), 3),
            (Has("::"), 1),
        ],
    },
    Rule {
        label: "C",
        tokens: &[
            (Starts("#include <"), 2),
            (Has("int main("), 3),
            (Has("printf("), 3),
            (Starts("#define "), 2),
            (Has("malloc("), 3),
            (Has("->"), 1),
            (Starts("typedef "), 2),
            (Has("#include <stdio.h>"), 3),
            (Has("size_t "), 1),
        ],
    },
    Rule {
        label: "Batch",
        tokens: &[
            (Starts("@echo off"), 5),
            (Starts("::"), 1),
            (Starts("rem "), 2),
            (Has("%~dp0"), 4),
            (Starts("goto "), 2),
            (Has("set /p"), 3),
            (Starts("setlocal"), 3),
            (Has("errorlevel"), 2),
            (Starts("echo."), 1),
        ],
    },
    Rule {
        label: "PowerShell",
        tokens: &[
            (Has("$_"), 1),
            (Has("Write-Host"), 4),
            (Has("Get-"), 3),
            (Has("Set-"), 2),
            (Starts("param("), 3),
            (Has(" -eq "), 2),
            (Has("$env:"), 3),
            (Has(" | Where-Object"), 4),
            (Has("-ErrorAction"), 3),
            (Has("[CmdletBinding"), 4),
        ],
    },
    Rule {
        label: "Shell",
        tokens: &[
            (Starts("echo "), 1),
            (Ends("then"), 2),
            (Starts("fi"), 2),
            (Starts("done"), 2),
            (Has("$("), 1),
            (Starts("export "), 2),
            (Starts("if ["), 3),
            (Has(" ]; then"), 3),
            (Has("$1"), 1),
            (Starts("sudo "), 2),
            (Has("/dev/null"), 3),
            (Has("; do"), 3),
        ],
    },
    Rule {
        label: "Ruby",
        tokens: &[
            (Starts("def "), 1),
            (Starts("end"), 2),
            (Starts("puts "), 2),
            (Starts("require '"), 3),
            (Has("attr_accessor"), 4),
            (Has(" do |"), 3),
            (Starts("module "), 1),
            (Starts("class "), 1),
            (Has(".each do"), 3),
        ],
    },
    Rule {
        label: "Lua",
        tokens: &[
            (Starts("local "), 3),
            (Has("function "), 1),
            (Starts("end"), 1),
            (Ends("then"), 1),
            (Starts("--"), 2),
            (Has("print("), 1),
            (Has("~="), 2),
            (Has("ipairs("), 4),
            (Has("pairs("), 2),
            (Has("require(\""), 1),
        ],
    },
    Rule {
        label: "Perl",
        tokens: &[
            (Starts("use strict"), 5),
            (Starts("my $"), 3),
            (Starts("sub "), 2),
            (Has("=~"), 3),
            (Has("$_"), 1),
            (Has("@ARGV"), 4),
            (Starts("use warnings"), 4),
            (Has("print \""), 1),
        ],
    },
    Rule {
        label: "PHP",
        tokens: &[
            (Has("<?php"), 6),
            (Has("->"), 1),
            (Has("echo $"), 3),
            (Has("function ($"), 1),
            (Has("$this->"), 4),
            (Has("namespace App"), 3),
            (Has("$_POST"), 4),
            (Has("$_GET"), 4),
        ],
    },
];
