#!/usr/bin/perl
use strict;
use warnings;

my $id = $ARGV[0] || 42;
my $payload = "";
while (<STDIN>) {
    chomp;
    $payload .= $_;
}
$payload = "none" if $payload eq "";

# Pass ID and the raw payload directly to Lua
print "$id,$payload\n";
