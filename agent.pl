#!/usr/bin/perl
use strict;
use warnings;

my $id = $ARGV[0] || 42;
my $raw_val = $id * 0.07;
print "$id,$raw_val\n";
